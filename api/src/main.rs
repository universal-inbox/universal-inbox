use clap::{Parser, Subcommand};
use sqlx::PgPool;
use std::{net::TcpListener, str::FromStr, sync::Arc};
use tracing::{error, info};
use universal_inbox_api::{
    commands,
    configuration::Settings,
    integrations::github::GithubService,
    observability::{get_subscriber, init_subscriber},
    repository::database::PgRepository,
    run,
    universal_inbox::notification::{service::NotificationService, source::NotificationSource},
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
        source: Option<NotificationSource>,
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
    let connection = PgPool::connect(&settings.database.connection_string())
        .await
        .expect("Failed to connect to Postgresql");
    let repository = Box::new(PgRepository::new(connection));
    let service = Arc::new(
        NotificationService::new(
            repository,
            GithubService::new(&settings.integrations.github.api_token, None)
                .expect("Failed to create new GithubService"),
            settings.integrations.github.page_size,
        )
        .expect("Failed to setup notification service"),
    );

    let result = match &cli.command {
        Commands::Sync { source } => commands::sync::sync(service, source).await,
        Commands::Serve => {
            let listener = TcpListener::bind(format!("0.0.0.0:{}", settings.application.port))
                .expect("Failed to bind port");

            let _ = run(listener, &settings, service)
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
