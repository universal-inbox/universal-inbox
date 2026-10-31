use std::str::FromStr;

use anyhow::Context;
use clap::{ArgGroup, Parser, Subcommand};
use email_address::EmailAddress;
use futures::future;
use std::{net::TcpListener, sync::Arc, time::Duration};
use tokio::sync::RwLock;
use universal_inbox::pii::Pii;

use universal_inbox::{
    audit::AuditActor,
    integration_connection::provider::IntegrationProviderKind,
    notification::{NotificationSourceKind, NotificationSyncSourceKind},
    task::TaskSyncSourceKind,
    user::UserId,
};

use crate::{
    configuration::Settings,
    integrations::slack::SlackService,
    jobs::connect_job_storage,
    run_ping_server, run_server, run_worker,
    universal_inbox::{
        UniversalInboxError,
        audit::{actor_context::with_actor, service::AuditService},
        auth_token::service::AuthenticationTokenService,
        integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService,
        oauth2::service::OAuth2Service,
        realtime::{build_event_publisher, with_event_scope},
        slack_bridge::service::SlackBridgeService,
        task::service::TaskService,
        third_party::service::ThirdPartyItemService,
        user::service::UserService,
    },
    utils::{cache::Cache, jwt::JWTBase64EncodedSigningKeys},
};

pub mod anonymize;
pub mod data_encryption;
pub mod generate;
pub mod integration_connection;
pub mod jobs;
pub mod oauth;
#[cfg(feature = "screenshots")]
pub mod screenshots;
pub mod slack;
pub mod sync;
pub mod todoist;
pub mod user;

async fn shutdown_signal() -> std::io::Result<()> {
    use tokio::signal::unix::{SignalKind, signal};

    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;
    tokio::select! {
        _ = sigterm.recv() => {},
        _ = sigint.recv() => {},
    }
    Ok(())
}

/// Universal Inbox API server and associated commands
#[derive(Parser)]
#[clap(version, about, long_about = None)]
pub struct Cli {
    /// Increase logging verbosity
    #[clap(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Synchronize sources of notifications
    SyncNotifications {
        /// Sync notifications for given user
        #[clap(short, long)]
        user_id: Option<UserId>,
        #[clap(short, long, value_enum, value_parser)]
        source: Option<NotificationSyncSourceKind>,
    },

    /// Synchronize sources of tasks
    SyncTasks {
        /// Sync notifications for given user
        #[clap(short, long)]
        user_id: Option<UserId>,
        #[clap(short, long, value_enum, value_parser)]
        source: Option<TaskSyncSourceKind>,
    },

    /// Refresh expiring OAuth tokens (eager refresh strategy)
    RefreshOAuthTokens {
        /// Only refresh tokens for given provider
        #[clap(short, long, value_enum, value_parser)]
        provider_kind: Option<IntegrationProviderKind>,
        /// Refresh tokens expiring within N minutes (default: 10)
        #[clap(short, long, default_value = "10")]
        minutes_before_expiry: i64,
    },

    /// Generate a new JWT key pair (to be added to your configuration file)
    GenerateJWTKeyPair,

    /// Run API server
    Serve {
        /// Start ASYNC_WORKERS_COUNT asynchronous workers from the API server process
        #[arg(short, long)]
        async_workers_count: Option<usize>,
        /// Start asynchronous workers from the API server process (count depends on the available cores or the value of `--async-workers-count` option)
        #[arg(short, long)]
        embed_async_workers: bool,
    },

    /// Run asynchronous workers
    StartWorkers {
        /// Start COUNT asynchronous workers (if not set, workers count will depends on the available cores)
        #[arg(short, long)]
        count: Option<usize>,
    },

    /// Manage cached results
    Cache {
        #[clap(subcommand)]
        command: CacheCommands,
    },

    /// Manage test data
    Test {
        #[clap(subcommand)]
        command: TestCommands,
    },

    /// Manage test data
    User {
        #[clap(subcommand)]
        command: UserCommands,
    },

    /// Todoist maintenance tasks
    Todoist {
        #[clap(subcommand)]
        command: TodoistCommands,
    },

    /// Slack maintenance tasks
    Slack {
        #[clap(subcommand)]
        command: SlackCommands,
    },

    /// Integration connections maintenance tasks
    IntegrationConnection {
        #[clap(subcommand)]
        command: IntegrationConnectionCommands,
    },

    /// Maintain the data encrypted at rest (see doc/src/config/data_encryption.md)
    DataEncryption {
        /// Print plain lines instead of the live progress view and tables, even in a terminal
        /// (always the case when the output is not a terminal)
        #[arg(long)]
        plain: bool,
        #[clap(subcommand)]
        command: DataEncryptionCommands,
    },

    /// Manage Stripe-backed billing state (reconciliation, etc.).
    /// No-op when `[billing]` is absent from configuration.
    Billing {
        #[clap(subcommand)]
        action: crate::billing::commands::BillingAction,
    },

    /// List, requeue and delete the jobs that exhausted their attempts (dead letter queue).
    /// For the local dev stack or an explicit maintenance: never run it against the
    /// production Redis from a laptop.
    DeadJobs {
        #[clap(subcommand)]
        command: DeadJobsCommands,
    },
}

#[derive(Subcommand)]
pub enum DeadJobsCommands {
    /// List dead jobs, most recently dead first
    List {
        /// Only list the jobs of this type (eg. `SyncTasks`, or `undecodable` for the jobs
        /// whose payload does not decode)
        #[arg(short = 't', long)]
        job_type: Option<String>,
        /// Maximum number of jobs to list
        #[arg(short, long, default_value = "50")]
        limit: usize,
    },
    /// Put dead jobs back on the queue, their attempts reset. Jobs whose payload does not
    /// decode are not requeued: they would die again.
    #[command(group(ArgGroup::new("selection").required(true).args(["job_ids", "all"])))]
    Requeue {
        /// Ids of the dead jobs to requeue
        job_ids: Vec<String>,
        /// Requeue all dead jobs (asks for confirmation without `--job-type`)
        #[arg(short, long)]
        all: bool,
        /// Only requeue the jobs of this type (with `--all`)
        #[arg(short = 't', long, conflicts_with = "job_ids")]
        job_type: Option<String>,
        /// Print the jobs that would be requeued, without requeuing them
        #[arg(short, long)]
        dry_run: bool,
        /// Do not ask for confirmation
        #[arg(short, long)]
        yes: bool,
    },
    /// Delete dead jobs, after confirmation
    #[command(group(
        ArgGroup::new("selection").required(true).args(["job_ids", "older_than", "all"])
    ))]
    Cleanup {
        /// Ids of the dead jobs to delete
        job_ids: Vec<String>,
        /// Delete the jobs that died more than this duration ago (eg. `7d`, `12h`)
        #[arg(short, long, value_parser = humantime::parse_duration)]
        older_than: Option<Duration>,
        /// Delete all dead jobs
        #[arg(short, long)]
        all: bool,
        /// Only delete the jobs of this type (`undecodable` for the jobs whose payload does
        /// not decode), with `--older-than` or `--all`
        #[arg(short = 't', long, conflicts_with = "job_ids")]
        job_type: Option<String>,
        /// Print the jobs that would be deleted, without deleting them
        #[arg(short, long)]
        dry_run: bool,
        /// Do not ask for confirmation
        #[arg(short, long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum DataEncryptionCommands {
    /// Count the encrypted values per key id and the Google account emails not replaced by
    /// their blind index yet
    Status,
    /// Replace the Google account emails stored before encryption by their blind index, then
    /// check every encrypted value can be read with the configured keys. Run before starting
    /// the API and the workers (done by the container entrypoint).
    EncryptPlaintext {
        #[arg(short, long, default_value = "500")]
        batch_size: i64,
    },
    /// Re-encrypt with the active key the values encrypted with another key (key rotation)
    Reencrypt {
        #[arg(short, long, default_value = "500")]
        batch_size: i64,
    },
}

#[derive(Subcommand)]
pub enum CacheCommands {
    /// Clear cached results
    Clear {
        /// Clear cached results with given prefix (after `universal-inbox::cache` prefix)
        #[arg(short, long)]
        prefix: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum TestCommands {
    /// Generate testing user
    GenerateUser,
    /// Generate an empty user (no integrations, no notifications, no tasks)
    GenerateEmptyUser,
    /// Mark a single integration as Validated for an existing user (simulates a completed OAuth2 flow)
    ConnectIntegration {
        #[arg(short, long)]
        user_id: UserId,
        #[arg(short, long, value_enum, value_parser)]
        provider: IntegrationProviderKind,
    },
    /// Generate sample notifications (one per known integration) for an existing user
    GenerateNotifications {
        #[arg(short, long)]
        user_id: UserId,
        /// Restrict generation to these notification source kinds (repeatable).
        /// When omitted, every known source is generated.
        #[arg(short, long, value_enum)]
        source: Vec<NotificationSourceKind>,
    },
    /// Anonymize database (user emails, names, passwords) after restoring a production backup
    AnonymizeDb,
    /// Regenerate the documentation screenshots by driving a headless browser
    /// against the locally running Universal Inbox. Requires building with the
    /// `screenshots` cargo feature; called by `just doc update-screenshots`.
    #[cfg(feature = "screenshots")]
    GenerateDocScreenshots {
        /// Base URL of the running web frontend (e.g. http://localhost:8080)
        #[arg(long)]
        base_url: String,
        /// Path to the mdBook src directory that contains the image folders
        #[arg(long, default_value = "../doc/src")]
        output_dir: std::path::PathBuf,
        /// Comma-separated list of screenshot names to regenerate (default: all)
        #[arg(long, value_delimiter = ',')]
        only: Option<Vec<String>>,
        /// Skip deleting the generated test user at the end of the run.
        #[arg(long, default_value_t = false)]
        keep_user: bool,
    },
    /// Record the landing-page screencast to a `.webm` file (silent).
    /// Drives the browser through the demo scenario at deterministic timings.
    /// Voiceover + avatar PiP are layered on top in a separate post-production pass
    /// (see `web/screencasts/landing-page.md`).
    #[cfg(feature = "screenshots")]
    RecordLandingScreencast {
        /// Base URL of the running web frontend (e.g. http://localhost:8080)
        #[arg(long, default_value = "http://localhost:8080")]
        base_url: String,
        /// Output path for the recorded `.webm` (overwritten if it exists).
        #[arg(long, default_value = "./screen.webm")]
        output: std::path::PathBuf,
        /// Skip deleting the recording user at the end of the run.
        #[arg(long, default_value_t = false)]
        keep_user: bool,
    },
}

#[derive(Subcommand)]
pub enum TodoistCommands {
    /// Replace legacy (pre API v1) Todoist IDs of synced tasks with their
    /// API v1 IDs (one-off backfill)
    MigrateLegacyIds {
        /// Only migrate tasks of given user
        #[arg(short, long)]
        user_id: Option<UserId>,
        #[arg(short, long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
pub enum SlackCommands {
    /// Store the missing Slack workspace (`team_id`) context of Slack
    /// integration connections, using Slack `auth.test` (one-off backfill)
    BackfillTeamId {
        /// Only backfill integration connections of given user
        #[arg(short, long)]
        user_id: Option<UserId>,
        #[arg(short, long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
pub enum IntegrationConnectionCommands {
    /// Pause, without any email, the OAuth connections of users not active
    /// since `--inactive-before` and the connections failing for more than
    /// `--failing-threshold-days` (one-off, before enabling the
    /// `pause_integration_connections` cron, so that it only emails users
    /// who become inactive or whose connections start failing afterwards)
    PauseWithoutEmail {
        /// Pause the connections of users not active since this date-time
        /// (RFC 3339, e.g. `2026-10-05T12:00:00Z`)
        #[arg(short, long)]
        inactive_before: chrono::DateTime<chrono::Utc>,
        /// Pause the connections failing for more than this many days
        /// (default: `application.cron.pause_integration_connections.failing_threshold_days`)
        #[arg(short, long)]
        failing_threshold_days: Option<i64>,
        #[arg(short, long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
pub enum UserCommands {
    /// List users
    List,
    /// Delete user and its data
    Delete {
        #[arg(short, long)]
        user_id: UserId,
    },
    /// Send welcome and verification email to user
    SendVerificationEmail {
        user_email: Pii<EmailAddress>,
        #[arg(short, long)]
        dry_run: bool,
    },

    /// Send the password reset email to user
    SendPasswordResetEmail {
        user_email: Pii<EmailAddress>,
        #[arg(short, long)]
        dry_run: bool,
    },

    /// Reset or set user password directly from CLI
    ResetPassword { user_email: Pii<EmailAddress> },

    /// Generate a new JWT token for given user
    GenerateJWTToken { user_email: Pii<EmailAddress> },
}

impl Cli {
    pub fn service_name(&self) -> String {
        match self.command {
            Commands::StartWorkers { .. } => "universal-inbox-workers".to_string(),
            _ => "universal-inbox-api".to_string(),
        }
    }

    pub fn log_level(&self, settings: &Settings) -> (String, log::LevelFilter) {
        if let Commands::DataEncryption { plain, .. } = &self.command
            && self.quiets_info_logs(data_encryption::OutputMode::detect(*plain))
        {
            return (log::LevelFilter::Warn.to_string(), log::LevelFilter::Warn);
        }
        match self.verbose {
            1 => (log::LevelFilter::Info.to_string(), log::LevelFilter::Info),
            2 => (log::LevelFilter::Debug.to_string(), log::LevelFilter::Debug),
            _ if self.verbose > 1 => (log::LevelFilter::Trace.to_string(), log::LevelFilter::Trace),
            _ => match &self.command {
                Commands::User {
                    command: UserCommands::List,
                }
                | Commands::DeadJobs {
                    command: DeadJobsCommands::List { .. },
                } => (log::LevelFilter::Error.to_string(), log::LevelFilter::Error),
                _ => (
                    settings
                        .application
                        .observability
                        .logging
                        .log_directive
                        .to_string(),
                    log::LevelFilter::from_str(
                        &settings
                            .application
                            .observability
                            .logging
                            .dependencies_log_level,
                    )
                    .unwrap(),
                ),
            },
        }
    }

    /// Info logs are written to stdout and would tear the live view of the data encryption
    /// commands: keep them for `-v` and the plain output only
    fn quiets_info_logs(&self, data_encryption_mode: data_encryption::OutputMode) -> bool {
        matches!(self.command, Commands::DataEncryption { .. })
            && self.verbose == 0
            && data_encryption_mode == data_encryption::OutputMode::Interactive
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        settings: Settings,
        cache: Cache,
        notification_service: Arc<RwLock<NotificationService>>,
        task_service: Arc<RwLock<TaskService>>,
        user_service: Arc<UserService>,
        integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
        auth_token_service: Arc<RwLock<AuthenticationTokenService>>,
        third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
        slack_service: Arc<SlackService>,
        slack_bridge_service: Arc<SlackBridgeService>,
        oauth2_service: Arc<OAuth2Service>,
        audit_service: Arc<AuditService>,
        billing_service: Option<Arc<crate::billing::service::BillingService>>,
    ) -> Result<(), UniversalInboxError> {
        // The server and the workers set the actor and the realtime event scope of each
        // request and job
        let cli_scope = match self.audit_command_name() {
            Some(command) => Some((
                command,
                build_event_publisher(&settings.application.realtime, &cache),
            )),
            None => None,
        };
        let execution = self.execute_command(
            settings,
            cache,
            notification_service,
            task_service,
            user_service,
            integration_connection_service,
            auth_token_service,
            third_party_item_service,
            slack_service,
            slack_bridge_service,
            oauth2_service,
            audit_service,
            billing_service,
        );
        match cli_scope {
            Some((command, event_publisher)) => {
                with_event_scope(
                    event_publisher,
                    with_actor(AuditActor::Cli { command }, execution),
                )
                .await
            }
            None => execution.await,
        }
    }

    /// Name recorded as the audit actor of the changes a command makes, `None` for the
    /// long-running server and workers.
    fn audit_command_name(&self) -> Option<String> {
        match self.command {
            Commands::Serve { .. } | Commands::StartWorkers { .. } => None,
            // The subcommand as typed, eg. `sync-notifications`: the first non-flag argument
            _ => Some(
                std::env::args()
                    .skip(1)
                    .find(|arg| !arg.starts_with('-'))
                    .unwrap_or_else(|| "unknown".to_string()),
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_command(
        &self,
        settings: Settings,
        cache: Cache,
        notification_service: Arc<RwLock<NotificationService>>,
        task_service: Arc<RwLock<TaskService>>,
        user_service: Arc<UserService>,
        integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
        auth_token_service: Arc<RwLock<AuthenticationTokenService>>,
        third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
        slack_service: Arc<SlackService>,
        slack_bridge_service: Arc<SlackBridgeService>,
        oauth2_service: Arc<OAuth2Service>,
        audit_service: Arc<AuditService>,
        billing_service: Option<Arc<crate::billing::service::BillingService>>,
    ) -> Result<(), UniversalInboxError> {
        match &self.command {
            Commands::SyncNotifications { source, user_id } => {
                sync::sync_notifications_for_all_users(notification_service, *source, *user_id)
                    .await
            }

            Commands::SyncTasks { source, user_id } => {
                sync::sync_tasks_for_all_users(task_service, *source, *user_id).await
            }

            Commands::RefreshOAuthTokens {
                provider_kind,
                minutes_before_expiry,
            } => {
                oauth::refresh_oauth_tokens(
                    integration_connection_service,
                    *provider_kind,
                    *minutes_before_expiry,
                )
                .await
            }

            Commands::GenerateJWTKeyPair => {
                let jwt_signing_keys = JWTBase64EncodedSigningKeys::generate()
                    .expect("Failed to generate JWT signing keys");
                println!("JWT signing keys:");
                println!(
                    "application.http_session.jwt_secret_key={}",
                    jwt_signing_keys.secret_key
                );
                println!(
                    "application.http_session.jwt_public_key={}",
                    jwt_signing_keys.public_key
                );
                Ok(())
            }

            Commands::Serve {
                async_workers_count,
                embed_async_workers,
            } => {
                let redis_storage = connect_job_storage(&settings)
                    .await
                    .expect("Redis storage connection failed");

                let listener = TcpListener::bind(format!(
                    "{}:{}",
                    settings.application.listen_address, settings.application.listen_port
                ))
                .expect("Failed to bind port");

                let cron_settings = settings.application.cron.clone();
                let realtime_settings = settings.application.realtime.clone();
                let server = run_server(
                    listener,
                    redis_storage.clone(),
                    settings,
                    cache.clone(),
                    notification_service.clone(),
                    task_service.clone(),
                    user_service,
                    integration_connection_service.clone(),
                    auth_token_service,
                    third_party_item_service.clone(),
                    slack_bridge_service,
                    oauth2_service,
                    audit_service.clone(),
                    billing_service,
                )
                .await
                .expect("Failed to start HTTP server");

                if async_workers_count.is_some() || *embed_async_workers {
                    let event_publisher = build_event_publisher(&realtime_settings, &cache);
                    let worker = run_worker(
                        *async_workers_count,
                        redis_storage,
                        cron_settings,
                        cache,
                        event_publisher,
                        notification_service,
                        task_service,
                        integration_connection_service,
                        third_party_item_service,
                        slack_service,
                        audit_service,
                    )
                    .await;

                    future::try_join(server, worker.run_with_signal(shutdown_signal()))
                        .await
                        .expect("Failed to wait for Server and asynchronous Workers");
                } else {
                    server.await.expect("Failed to start HTTP server");
                }

                Ok(())
            }

            Commands::StartWorkers { count } => {
                // Workers decrypt OAuth tokens and use the DB/SMTP credentials:
                // apply the same committed-secrets guard as `run_server`.
                settings
                    .check_committed_secrets()
                    .map_err(|message| UniversalInboxError::Unexpected(anyhow::anyhow!(message)))?;

                let redis_storage = connect_job_storage(&settings)
                    .await
                    .expect("Redis storage connection failed");

                let worker_port = settings
                    .application
                    .worker_listen_port
                    .unwrap_or(settings.application.listen_port);
                let listener = TcpListener::bind(format!(
                    "{}:{}",
                    settings.application.listen_address, worker_port
                ))
                .expect("Failed to bind worker health-check port");

                let ping_server = run_ping_server(
                    listener,
                    cache.clone(),
                    integration_connection_service.clone(),
                )
                .await
                .expect("Failed to start worker health-check server");

                let event_publisher = build_event_publisher(&settings.application.realtime, &cache);
                let worker = run_worker(
                    *count,
                    redis_storage,
                    settings.application.cron.clone(),
                    cache,
                    event_publisher,
                    notification_service,
                    task_service,
                    integration_connection_service,
                    third_party_item_service,
                    slack_service,
                    audit_service,
                )
                .await;

                future::try_join(ping_server, worker.run_with_signal(shutdown_signal()))
                    .await
                    .expect(
                        "Failed to wait for worker health-check server and asynchronous Workers",
                    );

                Ok(())
            }

            Commands::Cache { command } => match command {
                CacheCommands::Clear { prefix } => {
                    cache.clear(prefix).await.expect("Failed to clear cache");
                    Ok(())
                }
            },

            Commands::Test { command } => match command {
                TestCommands::GenerateUser => {
                    let _email = generate::generate_testing_user(
                        user_service,
                        integration_connection_service,
                        notification_service,
                        task_service,
                        third_party_item_service,
                        settings,
                    )
                    .await?;
                    Ok(())
                }
                TestCommands::GenerateEmptyUser => {
                    let _email = generate::generate_empty_user(user_service).await?;
                    Ok(())
                }
                TestCommands::ConnectIntegration { user_id, provider } => {
                    generate::connect_integration_for_user(
                        user_service,
                        integration_connection_service,
                        settings,
                        *user_id,
                        *provider,
                    )
                    .await
                }
                TestCommands::GenerateNotifications { user_id, source } => {
                    generate::generate_notifications_for_user(
                        user_service,
                        integration_connection_service,
                        notification_service,
                        task_service,
                        third_party_item_service,
                        settings,
                        *user_id,
                        source.clone(),
                    )
                    .await
                }
                TestCommands::AnonymizeDb => anonymize::anonymize_database(user_service).await,
                #[cfg(feature = "screenshots")]
                TestCommands::GenerateDocScreenshots {
                    base_url,
                    output_dir,
                    only,
                    keep_user,
                } => {
                    screenshots::generate_doc_screenshots(
                        user_service,
                        integration_connection_service,
                        notification_service,
                        task_service,
                        third_party_item_service,
                        settings,
                        base_url.clone(),
                        output_dir.clone(),
                        only.clone(),
                        *keep_user,
                    )
                    .await
                }
                #[cfg(feature = "screenshots")]
                TestCommands::RecordLandingScreencast {
                    base_url,
                    output,
                    keep_user,
                } => {
                    screenshots::screencast::record_landing_screencast(
                        user_service,
                        integration_connection_service,
                        notification_service,
                        task_service,
                        third_party_item_service,
                        settings,
                        base_url.clone(),
                        output.clone(),
                        *keep_user,
                    )
                    .await
                }
            },

            Commands::User { command } => match command {
                UserCommands::List => user::list_users(user_service).await,

                UserCommands::Delete { user_id } => {
                    user::delete_user(user_service, billing_service, *user_id).await
                }

                UserCommands::SendVerificationEmail {
                    user_email,
                    dry_run,
                } => user::send_verification_email(user_service, user_email, *dry_run).await,

                UserCommands::SendPasswordResetEmail {
                    user_email,
                    dry_run,
                } => user::send_password_reset_email(user_service, user_email, *dry_run).await,

                UserCommands::ResetPassword { user_email } => {
                    user::reset_password(user_service, user_email).await
                }

                UserCommands::GenerateJWTToken { user_email } => {
                    user::generate_jwt_token(user_service, auth_token_service, user_email).await
                }
            },

            Commands::Todoist { command } => match command {
                TodoistCommands::MigrateLegacyIds { user_id, dry_run } => {
                    todoist::migrate_legacy_ids(
                        task_service,
                        integration_connection_service,
                        *user_id,
                        *dry_run,
                    )
                    .await
                }
            },

            Commands::Slack { command } => match command {
                SlackCommands::BackfillTeamId { user_id, dry_run } => slack::backfill_team_id(
                    integration_connection_service,
                    slack_service,
                    *user_id,
                    *dry_run,
                )
                .await
                .map(|_| ()),
            },

            Commands::IntegrationConnection { command } => match command {
                IntegrationConnectionCommands::PauseWithoutEmail {
                    inactive_before,
                    failing_threshold_days,
                    dry_run,
                } => integration_connection::pause_without_email(
                    integration_connection_service,
                    *inactive_before,
                    failing_threshold_days.unwrap_or(
                        settings
                            .application
                            .cron
                            .pause_integration_connections
                            .failing_threshold_days,
                    ),
                    *dry_run,
                )
                .await
                .map(|_| ()),
            },

            Commands::DataEncryption { plain, command } => {
                data_encryption::run(
                    &settings,
                    command,
                    data_encryption::OutputMode::detect(*plain),
                )
                .await
            }

            Commands::Billing { action } => {
                crate::billing::commands::run(action, billing_service).await
            }

            Commands::DeadJobs { command } => {
                let storage = connect_job_storage(&settings)
                    .await
                    .context("Failed to connect to the job queue")?;
                match command {
                    DeadJobsCommands::List { job_type, limit } => {
                        jobs::list_dead_jobs(&storage, job_type.as_deref(), *limit).await
                    }
                    DeadJobsCommands::Requeue {
                        job_ids,
                        job_type,
                        dry_run,
                        yes,
                        ..
                    } => {
                        jobs::requeue_dead_jobs(
                            &storage,
                            jobs::dead_job_selection(job_ids, None),
                            job_type.as_deref(),
                            *dry_run,
                            *yes,
                        )
                        .await
                    }
                    DeadJobsCommands::Cleanup {
                        job_ids,
                        older_than,
                        job_type,
                        dry_run,
                        yes,
                        ..
                    } => {
                        jobs::cleanup_dead_jobs(
                            &storage,
                            jobs::dead_job_selection(job_ids, *older_than),
                            job_type.as_deref(),
                            *dry_run,
                            *yes,
                        )
                        .await
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, data_encryption::OutputMode};

    #[test]
    fn test_data_encryption_quiets_info_logs_only_in_interactive_mode() {
        let cli = Cli::try_parse_from(["universal-inbox", "data-encryption", "reencrypt"]).unwrap();
        assert!(cli.quiets_info_logs(OutputMode::Interactive));
        assert!(!cli.quiets_info_logs(OutputMode::Plain));

        let verbose =
            Cli::try_parse_from(["universal-inbox", "-v", "data-encryption", "reencrypt"]).unwrap();
        assert!(!verbose.quiets_info_logs(OutputMode::Interactive));

        let other = Cli::try_parse_from(["universal-inbox", "user", "list"]).unwrap();
        assert!(!other.quiets_info_logs(OutputMode::Interactive));
    }

    #[test]
    fn test_dead_jobs_commands_require_one_selection() {
        let parse = |args: &[&str]| {
            Cli::try_parse_from(["universal-inbox", "dead-jobs"].iter().chain(args)).is_ok()
        };

        assert!(parse(&["requeue", "id1", "id2"]));
        assert!(parse(&["requeue", "--all", "--job-type", "SyncTasks"]));
        assert!(!parse(&["requeue"]));
        assert!(!parse(&["requeue", "id1", "--all"]));
        assert!(!parse(&["requeue", "id1", "--job-type", "SyncTasks"]));

        assert!(parse(&[
            "cleanup",
            "--older-than",
            "7d",
            "--job-type",
            "SyncTasks"
        ]));
        assert!(parse(&["cleanup", "--all", "--job-type", "undecodable"]));
        assert!(!parse(&["cleanup", "id1", "--job-type", "SyncTasks"]));
        assert!(parse(&["cleanup", "id1", "--dry-run"]));
        assert!(parse(&["cleanup", "--all", "--yes"]));
        assert!(!parse(&["cleanup"]));
        assert!(!parse(&["cleanup", "--all", "--older-than", "7d"]));
        assert!(!parse(&["cleanup", "--older-than", "a week"]));
    }
}
