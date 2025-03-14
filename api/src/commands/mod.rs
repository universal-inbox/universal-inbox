use std::str::FromStr;

use apalis_redis::RedisStorage;
use clap::{Parser, Subcommand};
use email_address::EmailAddress;
use futures::future;
use std::{net::TcpListener, sync::Arc};
use tokio::sync::RwLock;
use tracing::info;

use universal_inbox::{
    integration_connection::provider::IntegrationProviderKind,
    notification::NotificationSyncSourceKind, task::TaskSyncSourceKind, user::UserId,
};

use crate::{
    configuration::Settings,
    integrations::slack::SlackService,
    run_server, run_worker,
    universal_inbox::{
        auth_token::service::AuthenticationTokenService,
        integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService, task::service::TaskService,
        third_party::service::ThirdPartyItemService, user::service::UserService,
        UniversalInboxError,
    },
    utils::{cache::Cache, jwt::JWTBase64EncodedSigningKeys},
};

pub mod generate;
pub mod sync;
pub mod user;

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

    /// Synchronize registered OAuth scopes from Nango into the database
    SyncOauthScopes {
        /// Sync OAuth scopes for given user
        #[clap(short, long)]
        user_id: Option<UserId>,
        #[clap(short, long, value_enum, value_parser)]
        provider_kind: Option<IntegrationProviderKind>,
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
        user_email: EmailAddress,
        #[arg(short, long)]
        dry_run: bool,
    },

    /// Send the password reset email to user
    SendPasswordResetEmail {
        user_email: EmailAddress,
        #[arg(short, long)]
        dry_run: bool,
    },

    /// Generate a new JWT token for given user
    GenerateJWTToken { user_email: EmailAddress },
}

impl Cli {
    pub fn service_name(&self) -> String {
        match self.command {
            Commands::StartWorkers { .. } => "universal-inbox-workers".to_string(),
            _ => "universal-inbox-api".to_string(),
        }
    }

    pub fn log_level(&self, settings: &Settings) -> (String, log::LevelFilter) {
        match self.verbose {
            1 => (log::LevelFilter::Info.to_string(), log::LevelFilter::Info),
            2 => (log::LevelFilter::Debug.to_string(), log::LevelFilter::Debug),
            _ if self.verbose > 1 => (log::LevelFilter::Trace.to_string(), log::LevelFilter::Trace),
            _ => match &self.command {
                Commands::User {
                    command: UserCommands::List,
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

    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        settings: Settings,
        notification_service: Arc<RwLock<NotificationService>>,
        task_service: Arc<RwLock<TaskService>>,
        user_service: Arc<UserService>,
        integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
        auth_token_service: Arc<RwLock<AuthenticationTokenService>>,
        third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
        slack_service: Arc<SlackService>,
    ) -> Result<(), UniversalInboxError> {
        match &self.command {
            Commands::SyncNotifications { source, user_id } => {
                sync::sync_notifications_for_all_users(notification_service, *source, *user_id)
                    .await
            }

            Commands::SyncTasks { source, user_id } => {
                sync::sync_tasks_for_all_users(task_service, *source, *user_id).await
            }

            Commands::SyncOauthScopes {
                provider_kind,
                user_id,
            } => {
                sync::sync_oauth_scopes(integration_connection_service, *provider_kind, *user_id)
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
                info!(
                    "Connecting to Redis server for job queuing on {}",
                    &settings.redis.safe_connection_string()
                );
                let redis_storage = RedisStorage::new_with_config(
                    apalis_redis::connect(settings.redis.connection_string())
                        .await
                        .expect("Redis storage connection failed"),
                    apalis_redis::Config::default()
                        .set_namespace("universal-inbox:jobs:UniversalInboxJob"),
                );

                let listener = TcpListener::bind(format!(
                    "{}:{}",
                    settings.application.listen_address, settings.application.listen_port
                ))
                .expect("Failed to bind port");

                let server = run_server(
                    listener,
                    redis_storage.clone(),
                    settings,
                    notification_service.clone(),
                    task_service.clone(),
                    user_service,
                    integration_connection_service.clone(),
                    auth_token_service,
                    third_party_item_service.clone(),
                )
                .await
                .expect("Failed to start HTTP server");

                if async_workers_count.is_some() || *embed_async_workers {
                    let worker = run_worker(
                        *async_workers_count,
                        redis_storage,
                        notification_service,
                        task_service,
                        integration_connection_service,
                        third_party_item_service,
                        slack_service,
                    )
                    .await;

                    future::try_join(server, worker.run_with_signal(tokio::signal::ctrl_c()))
                        .await
                        .expect("Failed to wait for Server and asynchronous Workers");
                } else {
                    server.await.expect("Failed to start HTTP server");
                }

                Ok(())
            }

            Commands::StartWorkers { count } => {
                info!(
                    "Connecting to Redis server for job queuing on {}",
                    &settings.redis.safe_connection_string()
                );
                let redis_storage = RedisStorage::new_with_config(
                    apalis_redis::connect(settings.redis.connection_string())
                        .await
                        .expect("Redis storage connection failed"),
                    apalis_redis::Config::default()
                        .set_namespace("universal-inbox:jobs:UniversalInboxJob"),
                );

                let worker = run_worker(
                    *count,
                    redis_storage,
                    notification_service,
                    task_service,
                    integration_connection_service,
                    third_party_item_service,
                    slack_service,
                )
                .await;

                worker
                    .run_with_signal(tokio::signal::ctrl_c())
                    .await
                    .expect("Failed to run asynchronous Workers");

                Ok(())
            }

            Commands::Cache { command } => match command {
                CacheCommands::Clear { prefix } => {
                    let cache = Cache::new(settings.redis.connection_string())
                        .await
                        .expect("Failed to create cache");
                    cache.clear(prefix).await.expect("Failed to clear cache");
                    Ok(())
                }
            },

            Commands::Test { command } => match command {
                TestCommands::GenerateUser => {
                    generate::generate_testing_user(
                        user_service,
                        integration_connection_service,
                        notification_service,
                        task_service,
                        third_party_item_service,
                        settings,
                    )
                    .await
                }
            },

            Commands::User { command } => match command {
                UserCommands::List => user::list_users(user_service).await,

                UserCommands::Delete { user_id } => user::delete_user(user_service, *user_id).await,

                UserCommands::SendVerificationEmail {
                    user_email,
                    dry_run,
                } => user::send_verification_email(user_service, user_email, *dry_run).await,

                UserCommands::SendPasswordResetEmail {
                    user_email,
                    dry_run,
                } => user::send_password_reset_email(user_service, user_email, *dry_run).await,

                UserCommands::GenerateJWTToken { user_email } => {
                    user::generate_jwt_token(user_service, auth_token_service, user_email).await
                }
            },
        }
    }
}
