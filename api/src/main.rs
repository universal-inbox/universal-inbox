#![recursion_limit = "256"]
use std::sync::Arc;

use clap::Parser;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use wiremock::MockServer;

use universal_inbox_api::{
    ExecutionContext, build_services, commands,
    configuration::Settings,
    integrations::{
        github::GithubService, google_calendar::GoogleCalendarService,
        google_drive::GoogleDriveService, google_mail::GoogleMailService, linear::LinearService,
        slack::SlackService, ticktick::TickTickService, todoist::TodoistService,
    },
    mailer::{DisabledMailer, Mailer, SmtpMailer},
    observability::{
        get_subscriber, get_subscriber_with_telemetry, init_sentry, init_subscriber,
        install_panic_hook, shutdown_telemetry,
    },
    utils::{cache::Cache, passkey::build_webauthn},
};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    color_backtrace::install();

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let cli = commands::Cli::parse();

    let settings = Settings::new().expect("Cannot load Universal Inbox configuration");

    // Held until the end of `main`: dropping it flushes the pending events.
    // Installed after `color_backtrace` and before `install_panic_hook`, so a
    // panic is logged, then reported to Sentry, then printed.
    let sentry_guard = init_sentry(
        settings.application.observability.sentry.as_ref(),
        &settings.application.environment,
    );
    let sentry_enabled = sentry_guard.is_some();

    // Determine execution context based on the command
    let execution_context = match &cli.command {
        commands::Commands::Serve { .. } => ExecutionContext::Http,
        _ => ExecutionContext::Worker, // All other commands (sync operations, workers, etc.)
    };
    let (log_env_filter, dep_log_level_filter) = cli.log_level(&settings);
    let log_format = settings.application.observability.logging.format;
    if let Some(tracing_settings) = &settings.application.observability.tracing {
        init_subscriber(
            get_subscriber_with_telemetry(
                &settings.application.environment,
                &log_env_filter,
                tracing_settings,
                &cli.service_name(),
                settings.application.version.clone(),
                log_format,
                sentry_enabled,
            ),
            dep_log_level_filter,
        );
    } else {
        let subscriber = get_subscriber(&log_env_filter, log_format, sentry_enabled);
        init_subscriber(subscriber, dep_log_level_filter);
    };
    install_panic_hook();

    info!(
        "Connecting to PostgreSQL on {}",
        &settings.database.safe_connection_string()
    );
    let pool = Arc::new(
        settings
            .database
            .connect_pool(log::LevelFilter::Debug)
            .await
            .expect("Failed to connect to Postgresql"),
    );

    // One multiplexed Redis connection, shared by the services, the HTTP server and the
    // workers of this process.
    info!(
        "Connecting to Redis on {}",
        &settings.redis.safe_connection_string()
    );
    let cache = Cache::new(&settings.redis)
        .await
        .expect("Failed to connect to Redis");

    let mailer: Arc<RwLock<dyn Mailer + Send + Sync>> = match &settings.application.email {
        Some(email) => {
            info!(
                "Connecting to SMTP server on {}",
                email.safe_connection_string()
            );
            Arc::new(RwLock::new(
                SmtpMailer::build(
                    email.smtp_server.clone(),
                    email.smtp_port,
                    email.smtp_username.clone(),
                    email.smtp_password.clone(),
                    email
                        .from_header
                        .parse()
                        .expect("Failed to parse email settings `from_header`"),
                    email
                        .reply_to_header
                        .parse()
                        .expect("Failed to parse email settings `reply_to`"),
                )
                .expect("Failed to build an SmtpMailer"),
            ))
        }
        None => {
            info!("No email settings configured, emails are disabled");
            Arc::new(RwLock::new(DisabledMailer))
        }
    };
    info!("Starting server on {}", settings.application.front_base_url);
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
    let ticktick_mock_server = get_ticktick_mock_server(&settings).await;

    let (
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
    ) = build_services(
        pool,
        &settings,
        cache.clone(),
        github_mock_server.map(|mock| mock.uri()),
        linear_graphql_mock_server.map(|mock| mock.uri()),
        google_mail_mock_server.map(|mock| mock.uri()),
        google_drive_mock_server.map(|mock| mock.uri()),
        google_calendar_mock_server.map(|mock| mock.uri()),
        slack_mock_server.map(|mock| mock.uri()),
        todoist_mock_server.map(|mock| mock.uri()),
        ticktick_mock_server.map(|mock| mock.uri()),
        None,
        mailer,
        webauthn,
        execution_context,
    )
    .await;

    let result = cli
        .execute(
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
        )
        .await;
    if let Err(err) = &result {
        error!("universal-inbox failed: {err:?}");
    }
    // Export the last spans and logs: `execute` returns on a normal exit and
    // after a SIGTERM/SIGINT graceful stop. Blocking, hence off the runtime.
    tokio::task::spawn_blocking(shutdown_telemetry)
        .await
        .expect("Failed to shut telemetry down");

    match result {
        Err(err) => panic!("universal-inbox failed: {err:?}"),
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

async fn get_ticktick_mock_server(settings: &Settings) -> Option<MockServer> {
    if settings.application.dry_run {
        let mock_server = wiremock::MockServer::start().await;
        TickTickService::mock_all(&mock_server).await;
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
