use std::{net::TcpListener, str::FromStr, sync::Arc};

use apalis::redis::RedisStorage;
use clap::{Parser, Subcommand};
use email_address::EmailAddress;
use futures::future;
use opentelemetry::global;
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    ConnectOptions,
};
use tokio::sync::RwLock;
use tracing::{error, info};

use universal_inbox::{
    notification::{NotificationSourceKind, NotificationSyncSourceKind},
    task::TaskSyncSourceKind,
    user::UserId,
};

use universal_inbox_api::{
    build_services, commands,
    configuration::Settings,
    integrations::oauth2::NangoService,
    mailer::SmtpMailer,
    observability::{get_subscriber, get_subscriber_with_telemetry, init_subscriber},
    run_server, run_worker,
    utils::{cache::Cache, jwt::JWTBase64EncodedSigningKeys},
};

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

    /// Clear notifications details from the database. Useful when stored data is no longer valid.
    DeleteNotificationDetails {
        #[clap(short, long, value_enum, value_parser)]
        source: NotificationSourceKind,
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
        let subscriber = get_subscriber_with_telemetry(
            &settings.application.environment,
            log_env_filter,
            tracing_settings,
        );
        init_subscriber(subscriber, dep_log_level_filter);
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
            .connect_with(options)
            .await
            .expect("Failed to connect to Postgresql"),
    );

    info!(
        "Connecting to Nango on {}",
        &settings.integrations.oauth2.nango_base_url
    );
    let nango_service = NangoService::new(
        settings.integrations.oauth2.nango_base_url.clone(),
        &settings.integrations.oauth2.nango_secret_key,
    )
    .expect("Failed to create new GithubService");

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

    let (
        notification_service,
        task_service,
        user_service,
        integration_connection_service,
        auth_token_service,
    ) = build_services(
        pool,
        &settings,
        None,
        None,
        None,
        None,
        None,
        nango_service,
        mailer,
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
        Commands::DeleteNotificationDetails { source } => {
            commands::migration::delete_notification_details(notification_service, *source).await
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
                task_service,
                user_service,
                integration_connection_service,
                auth_token_service,
            )
            .await
            .expect("Failed to start HTTP server");

            if async_workers_count.is_some() || *embed_async_workers {
                let worker =
                    run_worker(*async_workers_count, redis_storage, notification_service).await;

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

            let worker = run_worker(*count, redis_storage, notification_service).await;

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
    global::shutdown_tracer_provider();

    match result {
        Err(err) => {
            error!("universal-inbox failed: {err:?}");
            panic!("universal-inbox failed: {err:?}")
        }
        Ok(_) => Ok(()),
    }
}
