use std::{env, fs, net::TcpListener, str::FromStr, sync::Arc};

use apalis::redis::RedisStorage;
use httpmock::MockServer;
use openidconnect::{ClientId, IntrospectionUrl, IssuerUrl};
use rstest::*;
use sqlx::{
    postgres::PgConnectOptions, ConnectOptions, Connection, Executor, PgConnection, PgPool,
};
use tokio::sync::RwLock;
use tracing::info;
use url::Url;
use uuid::Uuid;

use universal_inbox_api::{
    configuration::{
        AuthenticationSettings, LocalAuthenticationSettings, OIDCFlowSettings, Settings,
    },
    integrations::{oauth2::NangoService, slack::SlackService},
    jobs::UniversalInboxJob,
    observability::{get_subscriber, init_subscriber},
    repository::Repository,
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService, task::service::TaskService,
        third_party::service::ThirdPartyItemService, user::service::UserService,
    },
    utils::cache::Cache,
};

use crate::helpers::mailer::MailerStub;

pub mod auth;
pub mod integration_connection;
pub mod job;
pub mod mailer;
pub mod notification;
pub mod rest;
pub mod task;
pub mod user;

pub struct TestedApp {
    pub app_address: String,
    pub api_address: String,
    pub repository: Arc<Repository>,
    pub user_service: Arc<UserService>,
    pub task_service: Arc<RwLock<TaskService>>,
    pub notification_service: Arc<RwLock<NotificationService>>,
    pub integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    pub third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    pub slack_service: Arc<SlackService>,
    pub github_mock_server: MockServer,
    pub linear_mock_server: MockServer,
    pub google_calendar_mock_server: MockServer,
    pub google_mail_mock_server: MockServer,
    pub slack_mock_server: MockServer,
    pub todoist_mock_server: MockServer,
    pub oidc_issuer_mock_server: Option<MockServer>,
    pub nango_mock_server: MockServer,
    pub mailer_stub: Arc<RwLock<MailerStub>>,
    pub redis_storage: RedisStorage<UniversalInboxJob>,
    pub cache: Cache,
}

impl Drop for TestedApp {
    fn drop(&mut self) {
        let cache = self.cache.clone();
        tokio::spawn(async move {
            let _ = cache.clear(&None).await;
        });
    }
}

#[fixture]
#[once]
fn tracing_setup(settings: Settings) {
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
async fn db_connection(mut settings: Settings) -> Arc<PgPool> {
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
async fn redis_storage(settings: Settings) -> RedisStorage<UniversalInboxJob> {
    let mut config = apalis_redis::Config::default();
    config.set_queue_name_prefix(Some(Uuid::new_v4().to_string()));
    RedisStorage::new_with_config(
        apalis::redis::connect(settings.redis.connection_string())
            .await
            .expect("Redis storage connection failed"),
        config,
    )
}

#[fixture]
pub fn settings() -> Settings {
    Settings::new_from_file(Some("config/test".to_string()))
        .expect("Cannot load test configuration")
}

#[fixture]
pub async fn tested_app(
    mut settings: Settings,
    #[allow(unused, clippy::let_unit_value)] tracing_setup: (),
    #[future] db_connection: Arc<PgPool>,
    #[future] redis_storage: RedisStorage<UniversalInboxJob>,
) -> TestedApp {
    info!("Setting up server");
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind random port");
    let port = listener.local_addr().unwrap().port();

    let cache = Cache::new(settings.redis.connection_string())
        .await
        .expect("Failed to create cache");
    Cache::set_namespace(Uuid::new_v4().to_string()).await;

    // tag: New notification integration
    let github_mock_server = MockServer::start();
    let github_mock_server_url = &github_mock_server.base_url();
    let linear_mock_server = MockServer::start();
    let linear_mock_server_url = &linear_mock_server.base_url();
    let google_calendar_mock_server = MockServer::start();
    let google_calendar_mock_server_url = &google_calendar_mock_server.base_url();
    let google_mail_mock_server = MockServer::start();
    let google_mail_mock_server_url = &google_mail_mock_server.base_url();
    let slack_mock_server = MockServer::start();
    let slack_mock_server_url = &slack_mock_server.base_url();
    let todoist_mock_server = MockServer::start();
    let todoist_mock_server_url = &todoist_mock_server.base_url();

    let oidc_issuer_mock_server = MockServer::start();
    let oidc_issuer_mock_server_url = &oidc_issuer_mock_server.base_url();
    let nango_mock_server = MockServer::start();
    let nango_mock_server_url = &nango_mock_server.base_url();

    if let AuthenticationSettings::OpenIDConnect(ref mut oidc_settings) =
        &mut settings.application.security.authentication[0]
    {
        oidc_settings.oidc_issuer_url =
            IssuerUrl::new(oidc_issuer_mock_server_url.to_string()).unwrap();
        if let OIDCFlowSettings::AuthorizationCodePKCEFlow(ref mut flow_settings) =
            oidc_settings.oidc_flow_settings
        {
            flow_settings.introspection_url =
                IntrospectionUrl::new(format!("{oidc_issuer_mock_server_url}/introspect")).unwrap();
            flow_settings.front_client_id = ClientId::new("12345".to_string());
        }
    }

    let pool: Arc<PgPool> = db_connection.await;

    let nango_service = NangoService::new(
        nango_mock_server_url.parse::<Url>().unwrap(),
        &settings.oauth2.nango_secret_key,
    )
    .expect("Failed to create new NangoService");

    let mailer_stub = Arc::new(RwLock::new(MailerStub::new()));
    let (
        notification_service,
        task_service,
        user_service,
        integration_connection_service,
        auth_token_service,
        third_party_item_service,
        slack_service,
    ) = universal_inbox_api::build_services(
        pool.clone(),
        &settings,
        Some(github_mock_server_url.to_string()),
        Some(linear_mock_server_url.to_string()),
        Some(google_mail_mock_server_url.to_string()),
        Some(google_calendar_mock_server_url.to_string()),
        Some(slack_mock_server_url.to_string()),
        Some(todoist_mock_server_url.to_string()),
        nango_service,
        mailer_stub.clone(),
    )
    .await;

    let redis_storage = redis_storage.await;

    let app_address = format!("http://127.0.0.1:{port}");
    let api_address = format!("{app_address}{}", settings.application.api_path);
    let server = universal_inbox_api::run_server(
        listener,
        redis_storage.clone(),
        settings,
        notification_service.clone(),
        task_service.clone(),
        user_service.clone(),
        integration_connection_service.clone(),
        auth_token_service,
        third_party_item_service.clone(),
    )
    .await
    .expect("Failed to bind address");

    tokio::spawn(server);

    let worker = universal_inbox_api::run_worker(
        Some(1),
        redis_storage.clone(),
        notification_service.clone(),
        task_service.clone(),
        integration_connection_service.clone(),
        third_party_item_service.clone(),
        slack_service.clone(),
    )
    .await;

    tokio::spawn(worker.run());

    let repository = Arc::new(Repository::new(pool.clone()));

    TestedApp {
        app_address,
        api_address,
        repository,
        user_service,
        task_service,
        notification_service,
        integration_connection_service,
        third_party_item_service,
        slack_service,
        github_mock_server,
        linear_mock_server,
        google_calendar_mock_server,
        google_mail_mock_server,
        slack_mock_server,
        todoist_mock_server,
        oidc_issuer_mock_server: Some(oidc_issuer_mock_server),
        nango_mock_server,
        mailer_stub,
        redis_storage,
        cache,
    }
}

#[fixture]
pub async fn tested_app_with_local_auth(
    mut settings: Settings,
    #[allow(unused, clippy::let_unit_value)] tracing_setup: (),
    #[future] db_connection: Arc<PgPool>,
    #[future] redis_storage: RedisStorage<UniversalInboxJob>,
) -> TestedApp {
    info!("Setting up server");
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind random port");
    let port = listener.local_addr().unwrap().port();

    let cache = Cache::new(settings.redis.connection_string())
        .await
        .expect("Failed to create cache");
    Cache::set_namespace(Uuid::new_v4().to_string()).await;

    // tag: New notification integration
    let github_mock_server = MockServer::start();
    let github_mock_server_url = &github_mock_server.base_url();
    let linear_mock_server = MockServer::start();
    let linear_mock_server_url = &linear_mock_server.base_url();
    let google_mail_mock_server = MockServer::start();
    let google_mail_mock_server_url = &google_mail_mock_server.base_url();
    let google_calendar_mock_server = MockServer::start();
    let google_calendar_mock_server_url = &google_calendar_mock_server.base_url();
    let slack_mock_server = MockServer::start();
    let slack_mock_server_url = &slack_mock_server.base_url();
    let todoist_mock_server = MockServer::start();
    let todoist_mock_server_url = &todoist_mock_server.base_url();

    let nango_mock_server = MockServer::start();
    let nango_mock_server_url = &nango_mock_server.base_url();

    settings.application.security.authentication =
        vec![AuthenticationSettings::Local(LocalAuthenticationSettings {
            argon2_algorithm: argon2::Algorithm::Argon2id,
            argon2_version: argon2::Version::V0x13,
            argon2_memory_size: 20000,
            argon2_iterations: 2,
            argon2_parallelism: 1,
        })];

    let pool: Arc<PgPool> = db_connection.await;

    let nango_service = NangoService::new(
        nango_mock_server_url.parse::<Url>().unwrap(),
        &settings.oauth2.nango_secret_key,
    )
    .expect("Failed to create new NangoService");

    let mailer_stub = Arc::new(RwLock::new(MailerStub::new()));
    let (
        notification_service,
        task_service,
        user_service,
        integration_connection_service,
        auth_token_service,
        third_party_item_service,
        slack_service,
    ) = universal_inbox_api::build_services(
        pool.clone(),
        &settings,
        Some(github_mock_server_url.to_string()),
        Some(linear_mock_server_url.to_string()),
        Some(google_mail_mock_server_url.to_string()),
        Some(google_calendar_mock_server_url.to_string()),
        Some(slack_mock_server_url.to_string()),
        Some(todoist_mock_server_url.to_string()),
        nango_service,
        mailer_stub.clone(),
    )
    .await;

    let redis_storage = redis_storage.await;

    let app_address = format!("http://127.0.0.1:{port}");
    let api_address = format!("{app_address}{}", settings.application.api_path);
    let server = universal_inbox_api::run_server(
        listener,
        redis_storage.clone(),
        settings,
        notification_service.clone(),
        task_service.clone(),
        user_service.clone(),
        integration_connection_service.clone(),
        auth_token_service,
        third_party_item_service.clone(),
    )
    .await
    .expect("Failed to bind address");

    tokio::spawn(server);

    let worker = universal_inbox_api::run_worker(
        Some(1),
        redis_storage.clone(),
        notification_service.clone(),
        task_service.clone(),
        integration_connection_service.clone(),
        third_party_item_service.clone(),
        slack_service.clone(),
    )
    .await;

    tokio::spawn(worker.run());

    let repository = Arc::new(Repository::new(pool.clone()));

    TestedApp {
        app_address,
        api_address,
        repository,
        user_service,
        task_service,
        notification_service,
        integration_connection_service,
        third_party_item_service,
        slack_service,
        github_mock_server,
        linear_mock_server,
        google_calendar_mock_server,
        google_mail_mock_server,
        slack_mock_server,
        todoist_mock_server,
        oidc_issuer_mock_server: None,
        nango_mock_server,
        mailer_stub,
        redis_storage,
        cache,
    }
}

pub fn fixture_path(fixture_file_name: &str) -> String {
    format!(
        "{}/tests/api/fixtures/{fixture_file_name}",
        env::var("CARGO_MANIFEST_DIR").unwrap()
    )
}

pub fn load_json_fixture_file<T: for<'de> serde::de::Deserialize<'de>>(
    fixture_file_name: &str,
) -> T {
    let input_str = fs::read_to_string(fixture_path(fixture_file_name)).unwrap();
    serde_json::from_str::<T>(&input_str).unwrap()
}
