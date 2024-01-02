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
    dev::{Server, Service},
    http::{self, header},
    middleware, web, App, HttpServer,
};
use actix_web_lab::web::spa;
use anyhow::Context;
use configuration::AuthenticationSettings;
use csp::{Directive, Source, Sources, CSP};
use mailer::Mailer;
use sqlx::PgPool;
use tokio::sync::RwLock;
use tracing::info;
use tracing_actix_web::TracingLogger;

use crate::{
    configuration::Settings,
    integrations::{
        github::GithubService, google_mail::GoogleMailService, linear::LinearService,
        oauth2::NangoService, todoist::TodoistService,
    },
    observability::AuthenticatedRootSpanBuilder,
    repository::Repository,
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService, task::service::TaskService,
        user::service::UserService, UniversalInboxError,
    },
};

pub mod commands;
pub mod configuration;
pub mod integrations;
pub mod mailer;
pub mod observability;
pub mod repository;
pub mod routes;
pub mod universal_inbox;
pub mod utils;

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
    let csp_header_value = build_csp_header(&settings);
    let settings_web_data = web::Data::new(settings);

    info!("Connecting to Redis on {}", redis_connection_string);
    let redis_store = RedisSessionStore::new(redis_connection_string.clone()).await?;

    info!("Listening on {}", listen_address);

    let server = HttpServer::new(move || {
        info!("Mounting API on {}", api_path);

        let api_scope = web::scope(api_path.trim_end_matches('/'))
            .route("/front_config", web::get().to(routes::config::front_config))
            .service(routes::auth::scope())
            .service(routes::integration_connection::scope())
            .service(routes::notification::scope())
            .service(routes::task::scope())
            .service(routes::user::scope())
            .app_data(web::Data::new(notification_service.clone()))
            .app_data(web::Data::new(task_service.clone()))
            .app_data(web::Data::new(user_service.clone()))
            .app_data(web::Data::new(integration_connection_service.clone()));

        let cors = Cors::default()
            .allowed_origin(&front_base_url)
            .allowed_methods(vec!["GET", "POST", "PATCH", "DELETE", "PUT"])
            .allowed_headers(vec![
                http::header::AUTHORIZATION,
                http::header::COOKIE,
                http::header::CONTENT_TYPE,
            ])
            .supports_credentials()
            .max_age(3600);

        let csp_header_value = csp_header_value.clone();
        let mut app = App::new()
            .wrap(cors)
            .wrap(TracingLogger::<AuthenticatedRootSpanBuilder>::new())
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
            .wrap_fn(move |req, srv| {
                let csp_header_value = csp_header_value.clone();
                let fut = srv.call(req);
                async move {
                    let mut res = fut.await?;
                    if res
                        .headers()
                        .get(header::CONTENT_TYPE)
                        .map(|value| {
                            value
                                .to_str()
                                .map(|s| s.starts_with("text/html"))
                                .unwrap_or_default()
                        })
                        .unwrap_or_default()
                    {
                        res.headers_mut().insert(
                            header::CONTENT_SECURITY_POLICY,
                            header::HeaderValue::from_str(&csp_header_value).unwrap(),
                        );
                    }
                    Ok(res)
                }
            })
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

#[allow(clippy::too_many_arguments)] // ignore for now, to revisit later
pub async fn build_services(
    pool: Arc<PgPool>,
    settings: &Settings,
    github_address: Option<String>,
    linear_graphql_url: Option<String>,
    google_mail_base_url: Option<String>,
    todoist_address: Option<String>,
    nango_service: NangoService,
    mailer: Arc<RwLock<dyn Mailer + Send + Sync>>,
) -> (
    Arc<RwLock<NotificationService>>,
    Arc<RwLock<TaskService>>,
    Arc<RwLock<UserService>>,
    Arc<RwLock<IntegrationConnectionService>>,
) {
    let repository = Arc::new(Repository::new(pool.clone()));

    let user_service = Arc::new(RwLock::new(UserService::new(
        repository.clone(),
        settings.application.clone(),
        mailer.clone(),
    )));

    let integration_connection_service = Arc::new(RwLock::new(IntegrationConnectionService::new(
        repository.clone(),
        nango_service,
        settings.integrations.oauth2.nango_provider_keys.clone(),
    )));

    let todoist_service =
        TodoistService::new(todoist_address, integration_connection_service.clone())
            .expect("Failed to create new TodoistService");
    // tag: New notification integration
    let github_service = GithubService::new(
        github_address,
        settings.integrations.github.page_size,
        integration_connection_service.clone(),
    )
    .expect("Failed to create new GithubService");
    let linear_service =
        LinearService::new(linear_graphql_url, integration_connection_service.clone())
            .expect("Failed to create new LinearService");

    let google_mail_service = Arc::new(RwLock::new(
        GoogleMailService::new(
            google_mail_base_url,
            settings.integrations.google_mail.page_size,
            integration_connection_service.clone(),
            Weak::new(),
        )
        .expect("Failed to create new GoogleMailService"),
    ));

    // tag: New notification integration
    let notification_service = Arc::new(RwLock::new(NotificationService::new(
        repository.clone(),
        github_service,
        linear_service,
        google_mail_service.clone(),
        Weak::new(),
        integration_connection_service.clone(),
        user_service.clone(),
        settings
            .application
            .min_sync_notifications_interval_in_minutes,
    )));

    google_mail_service
        .write()
        .await
        .set_notification_service(Arc::downgrade(&notification_service));

    let task_service = Arc::new(RwLock::new(TaskService::new(
        repository,
        todoist_service.clone(),
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

fn build_csp_header(settings: &Settings) -> String {
    let nango_ws_scheme = if settings.integrations.oauth2.nango_base_url.scheme() == "http" {
        "ws"
    } else {
        "wss"
    };
    let nango_base_url = settings.integrations.oauth2.nango_base_url.to_string();
    let mut nango_ws_base_url = settings.integrations.oauth2.nango_base_url.clone();
    nango_ws_base_url.set_scheme(nango_ws_scheme).unwrap();
    let mut connect_srcs = Sources::new_with(Source::Self_)
        .push(Source::Host(nango_ws_base_url.as_str()))
        .push(Source::Host(&nango_base_url));
    if let AuthenticationSettings::OpenIDConnect(oidc_settings) =
        &settings.application.security.authentication
    {
        connect_srcs.push_borrowed(Source::Host(oidc_settings.oidc_issuer_url.as_str()));
    }
    for url in settings.application.security.csp_extra_connect_src.iter() {
        connect_srcs.push_borrowed(Source::Host(url));
    }

    CSP::new()
        .push(Directive::DefaultSrc(Sources::new_with(Source::Self_)))
        .push(Directive::ScriptSrc(
            Sources::new_with(Source::Self_)
                .push(Source::WasmUnsafeEval)
                .push(Source::UnsafeInline),
        ))
        .push(Directive::StyleSrc(
            Sources::new_with(Source::Self_).push(Source::UnsafeInline),
        ))
        .push(Directive::ObjectSrc(Sources::new()))
        .push(Directive::ConnectSrc(connect_srcs))
        .push(Directive::ImgSrc(
            Sources::new_with(Source::Self_)
                .push(Source::Host("https://secure.gravatar.com"))
                .push(Source::Host("https://avatars.githubusercontent.com"))
                .push(Source::Host("https://public.linear.app"))
                .push(Source::Scheme("data")), // Allow loading of inlined svg
        ))
        .push(Directive::WorkerSrc(Sources::new()))
        .to_string()
}
