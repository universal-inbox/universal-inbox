#![recursion_limit = "256"]
use std::sync::Arc;

use clap::Parser;
use sqlx::{
    ConnectOptions, Executor,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use wiremock::MockServer;

use universal_inbox_api::{
    ExecutionContext, build_services, commands,
    configuration::Settings,
    integrations::{
        github::GithubService, google_calendar::GoogleCalendarService,
        google_drive::GoogleDriveService, google_mail::GoogleMailService, linear::LinearService,
        oauth2::NangoService, slack::SlackService, todoist::TodoistService,
    },
    mailer::SmtpMailer,
    observability::{
        get_subscriber, get_subscriber_with_telemetry, get_subscriber_with_telemetry_and_logging,
        init_subscriber,
    },
    utils::passkey::build_webauthn,
};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    color_backtrace::install();

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let cli = commands::Cli::parse();

    let settings = Settings::new().expect("Cannot load Universal Inbox configuration");

    // Determine execution context based on the command
    let execution_context = match &cli.command {
        commands::Commands::Serve { .. } => ExecutionContext::Http,
        _ => ExecutionContext::Worker, // All other commands (sync operations, workers, etc.)
    };
    let (log_env_filter, dep_log_level_filter) = cli.log_level(&settings);
    if let Some(tracing_settings) = &settings.application.observability.tracing {
        let service_name = cli.service_name();
        if tracing_settings.is_stdout_logging_enabled {
            init_subscriber(
                get_subscriber_with_telemetry_and_logging(
                    &settings.application.environment,
                    &log_env_filter,
                    tracing_settings,
                    &service_name,
                    settings.application.version.clone(),
                ),
                dep_log_level_filter,
            );
        } else {
            init_subscriber(
                get_subscriber_with_telemetry(
                    &settings.application.environment,
                    &log_env_filter,
                    tracing_settings,
                    &service_name,
                    settings.application.version.clone(),
                ),
                dep_log_level_filter,
            );
        }
    } else {
        let subscriber = get_subscriber(&log_env_filter);
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
        warn!("Starting server in DRY RUN mode, write calls to integrations will be mocked");
    };
    let github_mock_server = get_github_mock_server(&settings).await;
    let linear_graphql_mock_server = get_linear_mock_server(&settings).await;
    let google_mail_mock_server = get_google_mail_mock_server(&settings).await;
    let google_drive_mock_server = get_google_drive_mock_server(&settings).await;
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
        subscription_service,
    ) = build_services(
        pool,
        &settings,
        github_mock_server.map(|mock| mock.uri()),
        linear_graphql_mock_server.map(|mock| mock.uri()),
        google_mail_mock_server.map(|mock| mock.uri()),
        google_drive_mock_server.map(|mock| mock.uri()),
        google_calendar_mock_server.map(|mock| mock.uri()),
        slack_mock_server.map(|mock| mock.uri()),
        todoist_mock_server.map(|mock| mock.uri()),
        nango_service,
        mailer,
        webauthn,
        execution_context,
    )
    .await;

    match cli
        .execute(
            settings,
            notification_service,
            task_service,
            user_service,
            integration_connection_service,
            auth_token_service,
            third_party_item_service,
            slack_service,
            subscription_service,
        )
        .await
    {
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

async fn get_google_drive_mock_server(settings: &Settings) -> Option<MockServer> {
    if settings.application.dry_run {
        let mock_server = wiremock::MockServer::start().await;
        GoogleDriveService::mock_all(&mock_server).await;
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
