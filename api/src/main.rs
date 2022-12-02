use std::{net::TcpListener, str::FromStr, sync::Arc};

use clap::{Parser, Subcommand};
use sqlx::{postgres::PgConnectOptions, ConnectOptions, PgPool};
use tracing::{error, info};

use universal_inbox_api::{
    commands,
    configuration::Settings,
    integrations::{github::GithubService, todoist::TodoistService},
    observability::{get_subscriber, init_subscriber},
    repository::{notification::NotificationRepository, task::TaskRepository},
    run,
    universal_inbox::{
        notification::{service::NotificationService, source::NotificationSourceKind},
        task::service::TaskService,
    },
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
    /// Synchronize sources of notification
    Sync {
        #[clap(arg_enum, value_parser)]
        source: Option<NotificationSourceKind>,
    },

    /// Run API server
    Serve,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    color_backtrace::install();

    let cli = Cli::parse();

    let settings = Settings::new().expect("Cannot load Universal Inbox configuration");
    let (log_level_filter, dep_log_level_filter) = match cli.verbose {
        1 => (log::LevelFilter::Info, log::LevelFilter::Info),
        2 => (log::LevelFilter::Debug, log::LevelFilter::Debug),
        _ if cli.verbose > 1 => (log::LevelFilter::Trace, log::LevelFilter::Trace),
        _ => (
            log::LevelFilter::from_str(&settings.application.log_directive)
                .unwrap_or(log::LevelFilter::Info),
            log::LevelFilter::from_str(&settings.application.dependencies_log_directive)
                .unwrap_or(log::LevelFilter::Error),
        ),
    };
    let subscriber = get_subscriber(log_level_filter.as_str());
    init_subscriber(subscriber, dep_log_level_filter);

    info!(
        "Connecting to PostgreSQL on {}",
        &settings.database.connection_string()
    );
    let mut options = PgConnectOptions::new()
        .username(&settings.database.username)
        .password(&settings.database.password)
        .host(&settings.database.host)
        .port(settings.database.port)
        .database(&settings.database.database_name);
    options.log_statements(log::LevelFilter::Debug);
    let pool = Arc::new(
        PgPool::connect_with(options)
            .await
            .expect("Failed to connect to Postgresql"),
    );

    let todoist_service = TodoistService::new(&settings.integrations.todoist.api_token, None)
        .expect("Failed to create new TodoistService");
    let notification_service = Arc::new(
        NotificationService::new(
            Box::new(NotificationRepository::new(pool.clone())),
            GithubService::new(
                &settings.integrations.github.api_token,
                None,
                settings.integrations.github.page_size,
            )
            .expect("Failed to create new GithubService"),
            todoist_service.clone(),
        )
        .expect("Failed to setup notification service"),
    );

    let task_service = Arc::new(
        TaskService::new(Box::new(TaskRepository::new(pool.clone())), todoist_service)
            .expect("Failed to setup task service"),
    );

    let result = match &cli.command {
        Commands::Sync { source } => commands::sync::sync(notification_service, source).await,
        Commands::Serve => {
            let listener = TcpListener::bind(format!("0.0.0.0:{}", settings.application.port))
                .expect("Failed to bind port");

            let _ = run(listener, &settings, notification_service, task_service)
                .await
                .expect("Failed to start HTTP server")
                .await;
            Ok(())
        }
    };

    match result {
        Err(err) => {
            error!("universal-inbox failed: {:?}", err);
            panic!("universal-inbox failed: {:?}", err)
        }
        Ok(_) => Ok(()),
    }
}
