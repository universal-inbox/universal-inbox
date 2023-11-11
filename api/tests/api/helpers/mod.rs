use std::{env, fs, net::TcpListener, str::FromStr, sync::Arc};

use httpmock::MockServer;
use openidconnect::{IntrospectionUrl, IssuerUrl};
use rstest::*;
use sqlx::{
    postgres::PgConnectOptions, ConnectOptions, Connection, Executor, PgConnection, PgPool,
};
use tracing::info;
use url::Url;
use uuid::Uuid;

use universal_inbox_api::{
    configuration::Settings,
    integrations::oauth2::NangoService,
    observability::{get_subscriber, init_subscriber},
    repository::Repository,
};

pub mod auth;
pub mod integration_connection;
pub mod notification;
pub mod rest;
pub mod task;

pub struct TestedApp {
    pub app_address: String,
    pub api_address: String,
    pub repository: Arc<Repository>,
    pub github_mock_server: MockServer,
    pub linear_mock_server: MockServer,
    pub google_mail_mock_server: MockServer,
    pub todoist_mock_server: MockServer,
    pub oidc_issuer_mock_server: MockServer,
    pub nango_mock_server: MockServer,
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
                .dependencies_log_directive,
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
pub fn settings() -> Settings {
    Settings::new_from_file(Some("config/test".to_string()))
        .expect("Cannot load test configuration")
}

#[fixture]
pub async fn tested_app(
    mut settings: Settings,
    #[allow(unused, clippy::let_unit_value)] tracing_setup: (),
    #[future] db_connection: Arc<PgPool>,
) -> TestedApp {
    info!("Setting up server");
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind random port");
    let port = listener.local_addr().unwrap().port();

    // tag: New notification integration
    let github_mock_server = MockServer::start();
    let github_mock_server_uri = &github_mock_server.base_url();
    let linear_mock_server = MockServer::start();
    let linear_mock_server_uri = &linear_mock_server.base_url();
    let google_mail_mock_server = MockServer::start();
    let google_mail_mock_server_uri = &google_mail_mock_server.base_url();
    let todoist_mock_server = MockServer::start();
    let todoist_mock_server_uri = &todoist_mock_server.base_url();

    let oidc_issuer_mock_server = MockServer::start();
    let oidc_issuer_mock_server_uri = &oidc_issuer_mock_server.base_url();
    let nango_mock_server = MockServer::start();
    let nango_mock_server_uri = &nango_mock_server.base_url();

    settings.application.security.authentication.oidc_issuer_url =
        IssuerUrl::new(oidc_issuer_mock_server_uri.to_string()).unwrap();
    settings
        .application
        .security
        .authentication
        .oidc_introspection_url =
        IntrospectionUrl::new(format!("{oidc_issuer_mock_server_uri}/introspect")).unwrap();

    let pool: Arc<PgPool> = db_connection.await;

    let nango_service = NangoService::new(
        nango_mock_server_uri.parse::<Url>().unwrap(),
        &settings.integrations.oauth2.nango_secret_key,
    )
    .expect("Failed to create new NangoService");

    let (notification_service, task_service, user_service, integration_connection_service) =
        universal_inbox_api::build_services(
            pool.clone(),
            &settings,
            Some(github_mock_server_uri.to_string()),
            Some(linear_mock_server_uri.to_string()),
            Some(google_mail_mock_server_uri.to_string()),
            Some(todoist_mock_server_uri.to_string()),
            nango_service,
        )
        .await;

    let app_address = format!("http://127.0.0.1:{port}");
    let api_address = format!("{app_address}{}", settings.application.api_path);
    let server = universal_inbox_api::run(
        listener,
        settings,
        notification_service,
        task_service,
        user_service,
        integration_connection_service,
    )
    .await
    .expect("Failed to bind address");

    tokio::spawn(server);

    let repository = Arc::new(Repository::new(pool.clone()));

    TestedApp {
        app_address,
        api_address,
        repository,
        github_mock_server,
        linear_mock_server,
        google_mail_mock_server,
        todoist_mock_server,
        oidc_issuer_mock_server,
        nango_mock_server,
    }
}

pub fn load_json_fixture_file<T: for<'de> serde::de::Deserialize<'de>>(
    project_file_path: &str,
) -> T {
    let fixture_path = format!(
        "{}{project_file_path}",
        env::var("CARGO_MANIFEST_DIR").unwrap()
    );
    let input_str = fs::read_to_string(fixture_path).unwrap();
    serde_json::from_str::<T>(&input_str).unwrap()
}
