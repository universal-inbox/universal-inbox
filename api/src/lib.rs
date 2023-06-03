#[macro_use]
extern crate macro_attr;

#[macro_use]
extern crate enum_derive;

use std::{net::TcpListener, sync::Arc, sync::Weak};

use actix_cors::Cors;
use actix_identity::IdentityMiddleware;
use actix_session::{
    config::{CookieContentSecurity, PersistentSession},
    storage::RedisSessionStore,
    SessionMiddleware,
};
use actix_web::{
    cookie::{time::Duration, Key},
    dev::Server,
    http, middleware, web, App, HttpServer,
};
use actix_web_lab::web::spa;
use anyhow::Context;
use configuration::Settings;
use integrations::{github::GithubService, oauth2::NangoService, todoist::TodoistService};
use repository::Repository;
use sqlx::PgPool;
use tokio::sync::RwLock;
use tracing::info;
use tracing_actix_web::TracingLogger;

use crate::universal_inbox::{
    integration_connection::service::IntegrationConnectionService,
    notification::service::NotificationService, task::service::TaskService,
    user::service::UserService, UniversalInboxError,
};

pub mod commands;
pub mod configuration;
pub mod integrations;
pub mod observability;
pub mod repository;
pub mod routes;
pub mod universal_inbox;

pub async fn run(
    listener: TcpListener,
    settings: Settings,
    notification_service: Arc<RwLock<NotificationService>>,
    task_service: Arc<RwLock<TaskService>>,
    user_service: Arc<RwLock<UserService>>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
) -> Result<Server, UniversalInboxError> {
    let api_path = settings.application.api_path.clone();
    let front_base_url = settings
        .application
        .front_base_url
        .as_str()
        .trim_end_matches('/')
        .to_string();
    let static_path = settings.application.static_path.clone();
    let static_dir = settings
        .application
        .static_dir
        .clone()
        .unwrap_or_else(|| ".".to_string());
    let listen_address = listener.local_addr().unwrap();
    let redis_connection_string = settings.redis.connection_string();
    let session_secret_key = Key::from(settings.application.http_session.secret_key.as_bytes());
    let max_age_days = settings.application.http_session.max_age_days;
    let max_age_inactive_days = settings.application.http_session.max_age_inactive_days;
    let settings_web_data = web::Data::new(settings);

    info!("Connecting to Redis on {}", redis_connection_string);
    let redis_store = RedisSessionStore::new(redis_connection_string.clone()).await?;

    info!("Listening on {}", listen_address);

    let server = HttpServer::new(move || {
        info!(
            "Mounting API on {}",
            if api_path.is_empty() { "/" } else { &api_path }
        );

        let api_scope = web::scope(&api_path)
            .route("/front_config", web::get().to(routes::config::front_config))
            .service(routes::auth::scope())
            .service(routes::notification::scope())
            .service(routes::task::scope())
            .service(routes::integration_connection::scope())
            .app_data(web::Data::new(notification_service.clone()))
            .app_data(web::Data::new(task_service.clone()))
            .app_data(web::Data::new(user_service.clone()))
            .app_data(web::Data::new(integration_connection_service.clone()));

        let cors = Cors::default()
            .allowed_origin(&front_base_url)
            .allowed_methods(vec!["GET", "POST", "PATCH", "DELETE"])
            .allowed_headers(vec![
                http::header::AUTHORIZATION,
                http::header::COOKIE,
                http::header::CONTENT_TYPE,
            ])
            .supports_credentials()
            .max_age(3600);

        let mut app = App::new()
            .wrap(cors)
            .wrap(TracingLogger::default())
            .wrap(middleware::Compress::default())
            .wrap(
                IdentityMiddleware::builder()
                    .login_deadline(Some(Duration::days(max_age_days).try_into().unwrap()))
                    .visit_deadline(Some(
                        Duration::days(max_age_inactive_days).try_into().unwrap(),
                    ))
                    .build(),
            )
            .wrap(
                SessionMiddleware::builder(redis_store.clone(), session_secret_key.clone())
                    .session_lifecycle(
                        PersistentSession::default().session_ttl(Duration::days(max_age_days)),
                    )
                    .cookie_content_security(CookieContentSecurity::Signed)
                    .build(),
            )
            .route("/ping", web::get().to(routes::health_check::ping))
            .service(api_scope)
            .app_data(settings_web_data.clone());

        if let Some(path) = &static_path {
            info!(
                "Mounting static files on {}",
                if path.is_empty() { "/" } else { path }
            );
            app = app.service(
                spa()
                    .index_file(format!("{static_dir}/index.html"))
                    .static_resources_mount(path.clone())
                    .static_resources_location(static_dir.clone())
                    .finish(),
            );
        }
        app
    })
    .keep_alive(http::KeepAlive::Timeout(
        Duration::seconds(60).try_into().unwrap(),
    ))
    .shutdown_timeout(60)
    .listen(listener)
    .context(format!("Failed to listen on {listen_address}"))?;

    Ok(server.run())
}

pub async fn build_services(
    pool: Arc<PgPool>,
    settings: &Settings,
    github_service: GithubService,
    todoist_service: TodoistService,
    nango_service: NangoService,
) -> (
    Arc<RwLock<NotificationService>>,
    Arc<RwLock<TaskService>>,
    Arc<RwLock<UserService>>,
    Arc<RwLock<IntegrationConnectionService>>,
) {
    let repository = Arc::new(Repository::new(pool.clone()));
    let user_service = Arc::new(RwLock::new(UserService::new(
        repository.clone(),
        settings.application.authentication.clone(),
    )));

    let integration_connection_service = Arc::new(RwLock::new(IntegrationConnectionService::new(
        repository.clone(),
        nango_service,
        settings.integrations.oauth2.nango_provider_keys.clone(),
    )));

    let notification_service = Arc::new(RwLock::new(NotificationService::new(
        repository.clone(),
        github_service,
        Weak::new(),
        integration_connection_service.clone(),
        user_service.clone(),
        settings
            .application
            .min_sync_notifications_interval_in_minutes,
    )));

    let task_service = Arc::new(RwLock::new(TaskService::new(
        repository,
        todoist_service,
        Arc::downgrade(&notification_service),
        integration_connection_service.clone(),
        user_service.clone(),
        settings.application.min_sync_tasks_interval_in_minutes,
    )));

    notification_service
        .write()
        .await
        .set_task_service(Arc::downgrade(&task_service));

    (
        notification_service,
        task_service,
        user_service,
        integration_connection_service,
    )
}
