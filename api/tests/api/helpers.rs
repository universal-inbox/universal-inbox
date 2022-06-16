#![allow(clippy::useless_conversion)]

use rstest::*;
use sqlx::{Connection, Executor, PgConnection, PgPool};
use std::net::TcpListener;
use std::sync::Arc;
use tracing::info;
use universal_inbox_api::configuration::Settings;
use universal_inbox_api::observability::{get_subscriber, init_subscriber};
use universal_inbox_api::repository::notification::PgRepository;
use universal_inbox_api::universal_inbox::notification_service::NotificationService;
use uuid::Uuid;

#[fixture]
#[once]
fn tracing_setup(settings: Settings) {
    info!("Setting up tracing");
    let subscriber = get_subscriber(&settings.application.log_directive);
    init_subscriber(subscriber);
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
pub async fn app_address(
    settings: Settings,
    #[allow(unused)] tracing_setup: (),
    #[future] db_connection: PgPool,
) -> String {
    info!("Setting up server");
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind random port");
    let port = listener.local_addr().unwrap().port();
    let repository = Box::new(PgRepository::new(db_connection.await.into())); // useless_conversion disabled here
    let service = Arc::new(NotificationService::new(repository));
    let server =
        universal_inbox_api::run(listener, &settings, service).expect("Failed to bind address");

    let _ = tokio::spawn(server);

    format!("http://127.0.0.1:{}", port)
}
