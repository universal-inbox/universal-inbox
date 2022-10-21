#![allow(clippy::useless_conversion)]

use httpmock::prelude::*;
use reqwest::Response;
use rstest::*;
use sqlx::{Connection, Executor, PgConnection, PgPool};
use std::net::TcpListener;
use std::sync::Arc;
use tracing::info;
use universal_inbox::{Notification, NotificationPatch};
use universal_inbox_api::configuration::Settings;
use universal_inbox_api::integrations::github::GithubService;
use universal_inbox_api::observability::{get_subscriber, init_subscriber};
use universal_inbox_api::repository::database::PgRepository;
use universal_inbox_api::universal_inbox::notification::service::NotificationService;
use uuid::Uuid;

pub struct TestedApp {
    pub app_address: String,
    pub github_mock_server: MockServer,
}

#[fixture]
#[once]
fn tracing_setup(settings: Settings) {
    info!("Setting up tracing");
    color_backtrace::install();

    let subscriber = get_subscriber(&settings.application.log_directive);
    init_subscriber(subscriber, log::LevelFilter::Error);
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

    let db_connection = PgPool::connect(&settings.database.connection_string())
        .await
        .expect("error");

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
    let repository = Box::new(PgRepository::new(db_connection.await.into())); // useless_conversion disabled here

    let mock_server = MockServer::start();
    let mock_server_uri = &mock_server.base_url();
    let service = Arc::new(
        NotificationService::new(
            repository,
            GithubService::new("test_token", Some(mock_server_uri.to_string())).unwrap_or_else(
                |_| {
                    panic!(
                        "Failed to setup Github service with mock server at {}",
                        mock_server_uri
                    )
                },
            ),
            2,
        )
        .expect("Failed to setup notification service"),
    );

    let server = universal_inbox_api::run(listener, &settings, service)
        .await
        .expect("Failed to bind address");

    let _ = tokio::spawn(server);

    TestedApp {
        app_address: format!("http://127.0.0.1:{}", port),
        github_mock_server: mock_server,
    }
}

pub async fn create_notification_response(
    app_address: &str,
    notification: &Notification,
) -> Response {
    reqwest::Client::new()
        .post(&format!("{}/notifications", &app_address))
        .json(notification)
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn create_notification(
    app_address: &str,
    notification: &Notification,
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

pub async fn list_notifications_response(app_address: &str) -> Response {
    reqwest::Client::new()
        .get(&format!("{}/notifications", &app_address))
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn list_notifications(app_address: &str) -> Box<Vec<Notification>> {
    list_notifications_response(app_address)
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
