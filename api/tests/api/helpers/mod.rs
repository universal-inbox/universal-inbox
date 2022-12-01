#![allow(clippy::useless_conversion)]

use std::{env, fs, net::TcpListener, str::FromStr, sync::Arc};

use format_serde_error::SerdeError;
use httpmock::prelude::*;
use reqwest::Response;
use rstest::*;
use serde_json::json;
use sqlx::{
    postgres::PgConnectOptions, ConnectOptions, Connection, Executor, PgConnection, PgPool,
};
use tracing::info;
use uuid::Uuid;

use universal_inbox::{Notification, NotificationPatch, NotificationStatus};
use universal_inbox_api::configuration::Settings;
use universal_inbox_api::integrations::github::GithubService;
use universal_inbox_api::integrations::todoist::TodoistService;
use universal_inbox_api::observability::{get_subscriber, init_subscriber};
use universal_inbox_api::repository::notification::NotificationRepository;
use universal_inbox_api::universal_inbox::notification::service::NotificationService;
use universal_inbox_api::universal_inbox::notification::source::NotificationSourceKind;

pub mod github;
pub mod todoist;

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
async fn db_connection(mut settings: Settings) -> PgPool {
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

    db_connection
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
    #[future] db_connection: PgPool,
) -> TestedApp {
    info!("Setting up server");
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind random port");
    let port = listener.local_addr().unwrap().port();
    let repository = Box::new(NotificationRepository::new(db_connection.await.into())); // useless_conversion disabled here

    let github_mock_server = MockServer::start();
    let github_mock_server_uri = &github_mock_server.base_url();
    let todoist_mock_server = MockServer::start();
    let todoist_mock_server_uri = &todoist_mock_server.base_url();
    let service = Arc::new(
        NotificationService::new(
            repository,
            GithubService::new(
                &settings.integrations.github.api_token,
                Some(github_mock_server_uri.to_string()),
                2,
            )
            .unwrap_or_else(|_| {
                panic!(
                    "Failed to setup Github service with mock server at {}",
                    github_mock_server_uri
                )
            }),
            TodoistService::new(
                &settings.integrations.todoist.api_token,
                Some(todoist_mock_server_uri.to_string()),
            )
            .unwrap_or_else(|_| {
                panic!(
                    "Failed to setup Todoist service with mock server at {}",
                    todoist_mock_server_uri
                )
            }),
        )
        .expect("Failed to setup notification service"),
    );

    let server = universal_inbox_api::run(listener, &settings, service)
        .await
        .expect("Failed to bind address");

    let _ = tokio::spawn(server);

    TestedApp {
        app_address: format!("http://127.0.0.1:{}", port),
        github_mock_server,
        todoist_mock_server,
    }
}

pub async fn create_notification_response(
    app_address: &str,
    notification: Box<Notification>,
) -> Response {
    reqwest::Client::new()
        .post(&format!("{}/notifications", &app_address))
        .json(&*notification)
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn create_notification(
    app_address: &str,
    notification: Box<Notification>,
) -> Box<Notification> {
    create_notification_response(app_address, notification)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}

pub async fn patch_notification_response(
    app_address: &str,
    notification_id: Uuid,
    patch: &NotificationPatch,
) -> Response {
    reqwest::Client::new()
        .patch(&format!(
            "{}/notifications/{}",
            &app_address, notification_id
        ))
        .json(patch)
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn patch_notification(
    app_address: &str,
    notification_id: Uuid,
    patch: &NotificationPatch,
) -> Box<Notification> {
    patch_notification_response(app_address, notification_id, patch)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}

pub async fn list_notifications_response(
    app_address: &str,
    status_filter: NotificationStatus,
    include_snoozed_notifications: bool,
) -> Response {
    let snoozed_notifications_parameter = if include_snoozed_notifications {
        "&include_snoozed_notifications=true"
    } else {
        ""
    };

    reqwest::Client::new()
        .get(&format!(
            "{app_address}/notifications?status={status_filter}{snoozed_notifications_parameter}"
        ))
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn list_notifications(
    app_address: &str,
    status_filter: NotificationStatus,
    include_snoozed_notifications: bool,
) -> Box<Vec<Notification>> {
    list_notifications_response(app_address, status_filter, include_snoozed_notifications)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}

pub async fn get_notification_response(app_address: &str, id: uuid::Uuid) -> Response {
    reqwest::Client::new()
        .get(&format!("{}/notifications/{}", &app_address, id))
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn get_notification(app_address: &str, id: uuid::Uuid) -> Box<Notification> {
    get_notification_response(app_address, id)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}

pub async fn sync_notifications_response(
    app_address: &str,
    source: Option<NotificationSourceKind>,
) -> Response {
    reqwest::Client::new()
        .post(&format!("{}/notifications/sync", &app_address))
        .json(
            &source
                .map(|src| json!({"source": src.to_string()}))
                .unwrap_or_else(|| json!({})),
        )
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn sync_notifications(
    app_address: &str,
    source: Option<NotificationSourceKind>,
) -> Vec<Notification> {
    sync_notifications_response(app_address, source)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
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
