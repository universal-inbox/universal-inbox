use sqlx::PgPool;
use std::net::TcpListener;
use std::sync::Arc;
use tracing::info;
use universal_inbox_api::configuration::Settings;
use universal_inbox_api::observability::{get_subscriber, init_subscriber};
use universal_inbox_api::repository::notification::PgRepository;
use universal_inbox_api::run;
use universal_inbox_api::universal_inbox::notification_service::NotificationService;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let settings = Settings::new().expect("Cannot load Universal Inbox configuration");
    let subscriber = get_subscriber(&settings.application.log_directive);
    init_subscriber(subscriber);

    info!(
        "Connecting to PostgreSQL on {}",
        &settings.database.connection_string()
    );
    let listener = TcpListener::bind(format!("0.0.0.0:{}", settings.application.port))
        .expect("Failed to bind port");

    let connection = PgPool::connect(&settings.database.connection_string())
        .await
        .expect("Failed to connect to Postgresql");
    let repository = Box::new(PgRepository::new(connection));
    let service = Arc::new(NotificationService::new(repository));

    run(listener, &settings, service)?.await
}
