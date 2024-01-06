use std::{net::TcpListener, str::FromStr, sync::Arc};

use clap::{Parser, Subcommand};
use email_address::EmailAddress;
use opentelemetry::global;
use sqlx::{postgres::PgConnectOptions, ConnectOptions, PgPool};
use tokio::sync::RwLock;
use tracing::{error, info};

use universal_inbox::{
    notification::{NotificationSourceKind, NotificationSyncSourceKind},
    task::TaskSyncSourceKind,
};

use universal_inbox_api::{
    build_services, commands,
    configuration::Settings,
    integrations::oauth2::NangoService,
    mailer::SmtpMailer,
    observability::{get_subscriber, get_subscriber_with_telemetry, init_subscriber},
    run,
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
        #[clap(value_enum, value_parser)]
        source: Option<NotificationSyncSourceKind>,
    },

    /// Synchronize sources of tasks
    SyncTasks {
        #[clap(value_enum, value_parser)]
        source: Option<TaskSyncSourceKind>,
    },

    /// Clear notifications details from the database. Useful when stored data is no longer valid.
    DeleteNotificationDetails {
        #[clap(value_enum, value_parser)]
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

    /// Run API server
    Serve,
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
        PgPool::connect_with(options)
            .await
            .expect("Failed to connect to Postgresql"),
    );

    let nango_service = NangoService::new(
        settings.integrations.oauth2.nango_base_url.clone(),
        &settings.integrations.oauth2.nango_secret_key,
    )
    .expect("Failed to create new GithubService");

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

    let (notification_service, task_service, user_service, integration_connection_service) =
        build_services(
            pool,
            &settings,
            None,
            None,
            None,
            None,
            nango_service,
            mailer,
        )
        .await;

    let result = match &cli.command {
        Commands::SyncNotifications { source } => {
            commands::sync::sync_notifications_for_all_users(notification_service, *source).await
        }
        Commands::SyncTasks { source } => {
            commands::sync::sync_tasks_for_all_users(task_service, *source).await
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
        Commands::Serve => {
            let listener = TcpListener::bind(format!(
                "{}:{}",
                settings.application.listen_address, settings.application.listen_port
            ))
            .expect("Failed to bind port");

            let _ = run(
                listener,
                settings,
                notification_service,
                task_service,
                user_service,
                integration_connection_service,
            )
            .await
            .expect("Failed to start HTTP server")
            .await;
            Ok(())
        }
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
