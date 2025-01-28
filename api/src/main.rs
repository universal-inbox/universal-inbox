#![recursion_limit = "256"]
use std::{net::TcpListener, str::FromStr, sync::Arc};

use apalis::redis::RedisStorage;
use clap::{Parser, Subcommand};
use email_address::EmailAddress;
use futures::future;
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    ConnectOptions, Executor,
};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use universal_inbox::{
    integration_connection::provider::IntegrationProviderKind,
    notification::NotificationSyncSourceKind, task::TaskSyncSourceKind, user::UserId,
};

use universal_inbox_api::{
    build_services, commands,
    configuration::Settings,
    integrations::{
        github::GithubService, google_calendar::GoogleCalendarService,
        google_mail::GoogleMailService, linear::LinearService, oauth2::NangoService,
        slack::SlackService, todoist::TodoistService,
    },
    mailer::SmtpMailer,
    observability::{
        get_subscriber, get_subscriber_with_telemetry, get_subscriber_with_telemetry_and_logging,
        init_subscriber,
    },
    run_server, run_worker,
    utils::{cache::Cache, jwt::JWTBase64EncodedSigningKeys, passkey::build_webauthn},
};
use wiremock::MockServer;

/// Universal Inbox API server and associated commands
#[derive(Parser)]
#[clap(version, about, long_about = None)]
struct Cli {
    /// Increase logging verbosity
    #[clap(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
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

    /// Generate a new JWT key pair (to be added to your configuration file)
    GenerateJWTKeyPair,

    /// Generate a new JWT token for given user
    GenerateJWTToken { user_email: EmailAddress },

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
}

#[derive(Subcommand)]
enum CacheCommands {
    /// Clear cached results
    Clear {
        /// Clear cached results with given prefix (after `universal-inbox::cache` prefix)
        #[arg(short, long)]
        prefix: Option<String>,
    },
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    color_backtrace::install();

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let cli = Cli::parse();

    let settings = Settings::new().expect("Cannot load Universal Inbox configuration");
    let (log_env_filter, dep_log_level_filter) = match cli.verbose {
        1 => (log::LevelFilter::Info.as_str(), log::LevelFilter::Info),
        2 => (log::LevelFilter::Debug.as_str(), log::LevelFilter::Debug),
        _ if cli.verbose > 1 => (log::LevelFilter::Trace.as_str(), log::LevelFilter::Trace),
        _ => (
            settings
                .application
                .observability
                .logging
                .log_directive
                .as_str(),
            log::LevelFilter::from_str(
                &settings
                    .application
                    .observability
                    .logging
                    .dependencies_log_level,
            )
            .unwrap(),
        ),
    };
    if let Some(tracing_settings) = &settings.application.observability.tracing {
        let service_name = match &cli.command {
            Commands::StartWorkers { .. } => "universal-inbox-workers",
            _ => "universal-inbox-api",
        };
        if tracing_settings.is_stdout_logging_enabled {
            init_subscriber(
                get_subscriber_with_telemetry_and_logging(
                    &settings.application.environment,
                    log_env_filter,
                    tracing_settings,
                    service_name,
                    settings.application.version.clone(),
                ),
                dep_log_level_filter,
            );
        } else {
            init_subscriber(
                get_subscriber_with_telemetry(
                    &settings.application.environment,
                    log_env_filter,
                    tracing_settings,
                    service_name,
                    settings.application.version.clone(),
                ),
                dep_log_level_filter,
            );
        }
    } else {
        let subscriber = get_subscriber(log_env_filter);
        init_subscriber(subscriber, dep_log_level_filter);
    };

    info!(
        "Connecting to PostgreSQL on {}",
        &settings.database.safe_connection_string()
    );
    let options = PgConnectOptions::new()
        .username(&settings.database.username)
        .password(&settings.database.password)
        .host(&settings.database.host)
        .port(settings.database.port)
        .database(&settings.database.database_name)
        .log_statements(log::LevelFilter::Debug);
    let pool = Arc::new(
        PgPoolOptions::new()
            .max_connections(settings.database.max_connections)
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    conn.execute("SET default_transaction_isolation TO 'read committed'")
                        .await?;
                    Ok(())
                })
            })
            .connect_with(options)
            .await
            .expect("Failed to connect to Postgresql"),
    );

    info!("Connecting to Nango on {}", &settings.oauth2.nango_base_url);
    let nango_service = NangoService::new(
        settings.oauth2.nango_base_url.clone(),
        &settings.oauth2.nango_secret_key,
    )
    .expect("Failed to create new NangoService");

    info!(
        "Connecting to SMTP server on {}",
        &settings.application.email.safe_connection_string()
    );
    let mailer = Arc::new(RwLock::new(
        SmtpMailer::build(
            settings.application.email.smtp_server.clone(),
            settings.application.email.smtp_port,
            settings.application.email.smtp_username.clone(),
            settings.application.email.smtp_password.clone(),
            settings
                .application
                .email
                .from_header
                .parse()
                .expect("Failed to parse email settings `from_header`"),
            settings
                .application
                .email
                .reply_to_header
                .parse()
                .expect("Failed to parse email settings `reply_to`"),
        )
        .expect("Failed to build an SmtpMailer"),
    ));
    let webauthn = Arc::new(
        build_webauthn(&settings.application.front_base_url)
            .expect("Failed to build a Webauthn context"),
    );

    if settings.application.dry_run {
        warn!("⚠️ Starting server in DRY RUN mode, write calls to integrations will be mocked ⚠️");
    };
    let github_mock_server = get_github_mock_server(&settings).await;
    let linear_graphql_mock_server = get_linear_mock_server(&settings).await;
    let google_mail_mock_server = get_google_mail_mock_server(&settings).await;
    let google_calendar_mock_server = get_google_calendar_mock_server(&settings).await;
    let slack_mock_server = get_slack_mock_server(&settings).await;
    let todoist_mock_server = get_todoist_mock_server(&settings).await;

    let (
        notification_service,
        task_service,
        user_service,
        integration_connection_service,
        auth_token_service,
        third_party_item_service,
        slack_service,
    ) = build_services(
        pool,
        &settings,
        github_mock_server.map(|mock| mock.uri()),
        linear_graphql_mock_server.map(|mock| mock.uri()),
        google_mail_mock_server.map(|mock| mock.uri()),
        google_calendar_mock_server.map(|mock| mock.uri()),
        slack_mock_server.map(|mock| mock.uri()),
        todoist_mock_server.map(|mock| mock.uri()),
        nango_service,
        mailer,
        webauthn,
    )
    .await;

    let result = match &cli.command {
        Commands::SyncNotifications { source, user_id } => {
            commands::sync::sync_notifications_for_all_users(
                notification_service,
                *source,
                *user_id,
            )
            .await
        }
        Commands::SyncTasks { source, user_id } => {
            commands::sync::sync_tasks_for_all_users(task_service, *source, *user_id).await
        }
        Commands::SyncOauthScopes {
            provider_kind,
            user_id,
        } => {
            commands::sync::sync_oauth_scopes(
                integration_connection_service,
                *provider_kind,
                *user_id,
            )
            .await
        }
        Commands::SendVerificationEmail {
            user_email,
            dry_run,
        } => commands::user::send_verification_email(user_service, user_email, *dry_run).await,
        Commands::SendPasswordResetEmail {
            user_email,
            dry_run,
        } => commands::user::send_password_reset_email(user_service, user_email, *dry_run).await,
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
        Commands::GenerateJWTToken { user_email } => {
            commands::user::generate_jwt_token(user_service, auth_token_service, user_email).await
        }
        Commands::Serve {
            async_workers_count,
            embed_async_workers,
        } => {
            info!(
                "Connecting to Redis server for job queuing on {}",
                &settings.redis.safe_connection_string()
            );
            let redis_storage = RedisStorage::new(
                apalis::redis::connect(settings.redis.connection_string())
                    .await
                    .expect("Redis storage connection failed"),
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
            let redis_storage = RedisStorage::new(
                apalis::redis::connect(settings.redis.connection_string())
                    .await
                    .expect("Redis storage connection failed"),
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
    };

    match result {
        Err(err) => {
            error!("universal-inbox failed: {err:?}");
            panic!("universal-inbox failed: {err:?}")
        }
        Ok(_) => Ok(()),
    }
}

async fn get_todoist_mock_server(settings: &Settings) -> Option<MockServer> {
    if settings.application.dry_run {
        let mock_server = wiremock::MockServer::start().await;
        TodoistService::mock_all(&mock_server).await;
        Some(mock_server)
    } else {
        None
    }
}

async fn get_slack_mock_server(settings: &Settings) -> Option<MockServer> {
    if settings.application.dry_run {
        let mock_server = wiremock::MockServer::start().await;
        SlackService::mock_all(&mock_server).await;
        Some(mock_server)
    } else {
        None
    }
}

async fn get_google_mail_mock_server(settings: &Settings) -> Option<MockServer> {
    if settings.application.dry_run {
        let mock_server = wiremock::MockServer::start().await;
        GoogleMailService::mock_all(&mock_server).await;
        Some(mock_server)
    } else {
        None
    }
}

async fn get_google_calendar_mock_server(settings: &Settings) -> Option<MockServer> {
    if settings.application.dry_run {
        let mock_server = wiremock::MockServer::start().await;
        GoogleCalendarService::mock_all(&mock_server).await;
        Some(mock_server)
    } else {
        None
    }
}

async fn get_linear_mock_server(settings: &Settings) -> Option<MockServer> {
    if settings.application.dry_run {
        let mock_server = wiremock::MockServer::start().await;
        LinearService::mock_all(&mock_server).await;
        Some(mock_server)
    } else {
        None
    }
}

async fn get_github_mock_server(settings: &Settings) -> Option<MockServer> {
    if settings.application.dry_run {
        let mock_server = wiremock::MockServer::start().await;
        GithubService::mock_all(&mock_server).await;
        Some(mock_server)
    } else {
        None
    }
}
