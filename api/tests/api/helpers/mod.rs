use std::{env, fs, net::TcpListener, str::FromStr, sync::Arc};

use format_serde_error::SerdeError;
use httpmock::MockServer;
use rstest::*;
use sqlx::{
    postgres::PgConnectOptions, ConnectOptions, Connection, Executor, PgConnection, PgPool,
};
use tracing::info;
use uuid::Uuid;

use universal_inbox_api::{
    configuration::Settings,
    integrations::{github::GithubService, todoist::TodoistService},
    observability::{get_subscriber, init_subscriber},
};

pub mod notification;
pub mod rest;
pub mod task;

pub struct TestedApp {
    pub app_address: String,
    pub github_mock_server: MockServer,
    pub todoist_mock_server: MockServer,
}

#[fixture]
#[once]
fn tracing_setup(settings: Settings) {
    info!("Setting up tracing");
    color_backtrace::install();

    let subscriber = get_subscriber(&settings.application.log_directive);
    init_subscriber(
        subscriber,
        log::LevelFilter::from_str(&settings.application.dependencies_log_directive)
            .unwrap_or(log::LevelFilter::Error),
    );
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

    let mut options = PgConnectOptions::new()
        .username(&settings.database.username)
        .password(&settings.database.password)
        .host(&settings.database.host)
        .port(settings.database.port)
        .database(&settings.database.database_name);
    options.log_statements(log::LevelFilter::Info);
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
    settings: Settings,
    #[allow(unused)] tracing_setup: (),
    #[future] db_connection: Arc<PgPool>,
) -> TestedApp {
    info!("Setting up server");
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind random port");
    let port = listener.local_addr().unwrap().port();

    let github_mock_server = MockServer::start();
    let github_mock_server_uri = &github_mock_server.base_url();
    let todoist_mock_server = MockServer::start();
    let todoist_mock_server_uri = &todoist_mock_server.base_url();
    let pool: Arc<PgPool> = db_connection.await;

    let todoist_service = TodoistService::new(
        &settings.integrations.todoist.api_token,
        Some(todoist_mock_server_uri.to_string()),
    )
    .unwrap_or_else(|_| {
        panic!(
            "Failed to setup Todoist service with mock server at {}",
            todoist_mock_server_uri
        )
    });
    let github_service = GithubService::new(
        &settings.integrations.github.api_token,
        Some(github_mock_server_uri.to_string()),
        2,
    )
    .expect("Failed to create new GithubService");

    let (notification_service, task_service) =
        universal_inbox_api::build_services(pool, github_service, todoist_service).await;

    let server = universal_inbox_api::run(listener, &settings, notification_service, task_service)
        .await
        .expect("Failed to bind address");

    tokio::spawn(server);

    TestedApp {
        app_address: format!("http://127.0.0.1:{}", port),
        github_mock_server,
        todoist_mock_server,
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
    serde_json::from_str::<T>(&input_str)
        .map_err(|err| SerdeError::new(input_str, err))
        .unwrap()
}
