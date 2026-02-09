use std::{net::TcpListener, str::FromStr, sync::Arc};

use apalis_redis::RedisStorage;
use rstest::*;
use sqlx::{
    ConnectOptions, Connection, Executor, PgConnection, PgPool, postgres::PgConnectOptions,
};
use tokio::sync::RwLock;
use tracing::info;
use url::Url;
use uuid::Uuid;
use wiremock::MockServer;

use universal_inbox_api::{
    configuration::Settings,
    integrations::{oauth2::NangoService, slack::SlackService},
    jobs::UniversalInboxJob,
    observability::{get_subscriber, init_subscriber},
    universal_inbox::{
        auth_token::service::AuthenticationTokenService,
        integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService, task::service::TaskService,
        third_party::service::ThirdPartyItemService, user::service::UserService,
    },
    utils::{cache::Cache, passkey::build_webauthn},
};

use crate::common::mailer::MailerStub;

pub mod mailer;

// ---------------------------------------------------------------------------
// rstest fixtures (shared between API and browser tests)
// ---------------------------------------------------------------------------

#[fixture]
#[once]
pub fn tracing_setup(settings: Settings) {
    info!("Setting up tracing");

    let subscriber = get_subscriber(&settings.application.observability.logging.log_directive);
    init_subscriber(
        subscriber,
        log::LevelFilter::from_str(
            &settings
                .application
                .observability
                .logging
                .dependencies_log_level,
        )
        .unwrap_or(log::LevelFilter::Error),
    );
    color_backtrace::install();
}

#[fixture]
pub async fn db_connection(mut settings: Settings) -> Arc<PgPool> {
    settings.database.database_name = Uuid::new_v4().to_string();
    let mut server_connection =
        PgConnection::connect(&settings.database.connection_string_without_db())
            .await
            .expect("Failed to connect to Postgres");
    server_connection
        .execute(&*format!(
            r#"CREATE DATABASE "{}";"#,
            settings.database.database_name
        ))
        .await
        .expect("Failed to create database.");

    let options = PgConnectOptions::new()
        .username(&settings.database.username)
        .password(&settings.database.password)
        .host(&settings.database.host)
        .port(settings.database.port)
        .database(&settings.database.database_name)
        .log_statements(log::LevelFilter::Info);
    let db_connection = PgPool::connect_with(options).await.expect("error");

    sqlx::migrate!("./migrations")
        .run(&db_connection)
        .await
        .expect("Failed to migrate the database");

    Arc::new(db_connection)
}

#[fixture]
pub async fn redis_storage(settings: Settings) -> RedisStorage<UniversalInboxJob> {
    let namespace = format!("universal-inbox:jobs:UniversalInboxJob:{}", Uuid::new_v4());
    RedisStorage::new_with_config(
        apalis_redis::connect(settings.redis.connection_string())
            .await
            .expect("Redis storage connection failed"),
        apalis_redis::Config::default().set_namespace(&namespace),
    )
}

#[fixture]
pub fn settings() -> Settings {
    Settings::new_from_file(Some("config/test".to_string()))
        .expect("Cannot load test configuration")
}

// ---------------------------------------------------------------------------
// Mock servers
// ---------------------------------------------------------------------------

pub struct MockServers {
    pub github: MockServer,
    pub linear: MockServer,
    pub google_calendar: MockServer,
    pub google_mail: MockServer,
    pub google_drive: MockServer,
    pub slack: MockServer,
    pub todoist: MockServer,
    pub nango: MockServer,
}

impl MockServers {
    pub async fn start() -> Self {
        // tag: New notification integration
        let github = MockServer::start().await;
        let linear = MockServer::start().await;
        let google_calendar = MockServer::start().await;
        let google_mail = MockServer::start().await;
        let google_drive = MockServer::start().await;
        let slack = MockServer::start().await;
        let todoist = MockServer::start().await;
        let nango = MockServer::start().await;

        Self {
            github,
            linear,
            google_calendar,
            google_mail,
            google_drive,
            slack,
            todoist,
            nango,
        }
    }
}

// ---------------------------------------------------------------------------
// Shared service builder
// ---------------------------------------------------------------------------

pub struct TestServices {
    pub notification_service: Arc<RwLock<NotificationService>>,
    pub task_service: Arc<RwLock<TaskService>>,
    pub user_service: Arc<UserService>,
    pub integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    pub third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    pub slack_service: Arc<SlackService>,
}

pub async fn build_test_services(
    pool: Arc<PgPool>,
    settings: &Settings,
    mock_servers: &MockServers,
    mailer: Arc<RwLock<dyn universal_inbox_api::mailer::Mailer + Send + Sync>>,
) -> (TestServices, Arc<RwLock<AuthenticationTokenService>>) {
    let webauthn = Arc::new(
        build_webauthn(&settings.application.front_base_url)
            .expect("Failed to build a Webauthn context"),
    );

    let nango_service = NangoService::new(
        mock_servers.nango.uri().parse::<Url>().unwrap(),
        &settings.oauth2.nango_secret_key,
    )
    .expect("Failed to create new NangoService");

    let (
        notification_service,
        task_service,
        user_service,
        integration_connection_service,
        auth_token_service,
        third_party_item_service,
        slack_service,
    ) = universal_inbox_api::build_services(
        pool,
        settings,
        Some(mock_servers.github.uri()),
        Some(mock_servers.linear.uri()),
        Some(mock_servers.google_mail.uri()),
        Some(mock_servers.google_drive.uri()),
        Some(mock_servers.google_calendar.uri()),
        Some(mock_servers.slack.uri()),
        Some(mock_servers.todoist.uri()),
        nango_service,
        mailer,
        webauthn,
        universal_inbox_api::ExecutionContext::Http,
    )
    .await;

    let services = TestServices {
        notification_service,
        task_service,
        user_service,
        integration_connection_service,
        third_party_item_service,
        slack_service,
    };

    (services, auth_token_service)
}

// ---------------------------------------------------------------------------
// Server & worker spawning
// ---------------------------------------------------------------------------

pub async fn spawn_test_server(
    listener: TcpListener,
    redis_storage: RedisStorage<UniversalInboxJob>,
    settings: Settings,
    services: &TestServices,
    auth_token_service: Arc<RwLock<AuthenticationTokenService>>,
) {
    let server = universal_inbox_api::run_server(
        listener,
        redis_storage,
        settings,
        services.notification_service.clone(),
        services.task_service.clone(),
        services.user_service.clone(),
        services.integration_connection_service.clone(),
        auth_token_service,
        services.third_party_item_service.clone(),
    )
    .await
    .expect("Failed to bind address");

    tokio::spawn(server);
}

pub async fn spawn_test_worker(
    redis_storage: RedisStorage<UniversalInboxJob>,
    services: &TestServices,
) {
    let worker = universal_inbox_api::run_worker(
        Some(1),
        redis_storage,
        services.notification_service.clone(),
        services.task_service.clone(),
        services.integration_connection_service.clone(),
        services.third_party_item_service.clone(),
        services.slack_service.clone(),
    )
    .await;

    tokio::spawn(worker.run());
}

// ---------------------------------------------------------------------------
// Common test environment setup
// ---------------------------------------------------------------------------

/// Sets up the common test environment: rustls, listener, cache, mock servers.
/// Returns (listener, port, cache, mock_servers).
pub async fn setup_test_env(settings: &Settings) -> (TcpListener, u16, Cache, MockServers) {
    // Use `let _ =` because `install_default` can only succeed once per process.
    // Subsequent calls (from other tests in the same binary) return Err, which is harmless.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind random port");
    let port = listener.local_addr().unwrap().port();

    let cache = Cache::new(settings.redis.connection_string())
        .await
        .expect("Failed to create cache");
    Cache::set_namespace(Uuid::new_v4().to_string()).await;

    let mock_servers = MockServers::start().await;

    (listener, port, cache, mock_servers)
}

/// Builds services, spawns server and worker. Returns (TestServices, mailer_stub, redis_storage).
pub async fn build_and_spawn(
    listener: TcpListener,
    pool: Arc<PgPool>,
    settings: Settings,
    mock_servers: &MockServers,
    redis_storage: RedisStorage<UniversalInboxJob>,
) -> (
    TestServices,
    Arc<RwLock<MailerStub>>,
    RedisStorage<UniversalInboxJob>,
) {
    let mailer_stub = Arc::new(RwLock::new(MailerStub::new()));
    let (services, auth_token_service) =
        build_test_services(pool, &settings, mock_servers, mailer_stub.clone()).await;

    spawn_test_server(
        listener,
        redis_storage.clone(),
        settings,
        &services,
        auth_token_service,
    )
    .await;

    spawn_test_worker(redis_storage.clone(), &services).await;

    (services, mailer_stub, redis_storage)
}
