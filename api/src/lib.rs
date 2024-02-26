use std::{net::TcpListener, num::NonZeroUsize, sync::Arc, sync::Weak, thread};

use actix_cors::Cors;
use actix_http::StatusCode;
use actix_jwt_authc::{
    AuthenticateMiddlewareFactory, AuthenticateMiddlewareSettings, JWTSessionKey,
};
use actix_session::{
    config::{CookieContentSecurity, PersistentSession},
    storage::CookieSessionStore,
    SessionMiddleware,
};
use actix_web::{
    cookie::{
        time::{Duration, OffsetDateTime},
        Cookie, Key, SameSite,
    },
    dev::{Server, Service, ServiceResponse},
    http::{self, header},
    middleware::{self, ErrorHandlerResponse, ErrorHandlers},
    web, App, HttpServer, Result as ActixResult,
};
use actix_web_lab::web::spa;
use actix_web_opentelemetry::RequestMetrics;
use anyhow::Context;
use apalis::{
    layers::tracing::{DefaultOnRequest, DefaultOnResponse, TraceLayer},
    prelude::*,
    redis::RedisStorage,
};
use configuration::AuthenticationSettings;
use csp::{Directive, Source, Sources, CSP};
use futures::channel::mpsc;
use jobs::slack::SlackPushEventCallbackJob;
use jsonwebtoken::{Algorithm, Validation};
use mailer::Mailer;
use sqlx::PgPool;
use tokio::sync::RwLock;
use tracing::{error, info, Level};
use tracing_actix_web::TracingLogger;

use crate::{
    configuration::Settings,
    integrations::{
        github::GithubService, google_mail::GoogleMailService, linear::LinearService,
        oauth2::NangoService, todoist::TodoistService,
    },
    jobs::slack::handle_slack_push_event,
    observability::AuthenticatedRootSpanBuilder,
    repository::Repository,
    universal_inbox::{
        auth_token::service::AuthenticationTokenService,
        integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService, task::service::TaskService,
        user::service::UserService, UniversalInboxError,
    },
    utils::jwt::{Claims, JWTBase64EncodedSigningKeys, JWTSigningKeys, JWT_SESSION_KEY},
};

pub mod commands;
pub mod configuration;
pub mod integrations;
pub mod jobs;
pub mod mailer;
pub mod observability;
pub mod repository;
pub mod routes;
pub mod universal_inbox;
pub mod utils;

#[allow(clippy::too_many_arguments)]
pub async fn run_server(
    listener: TcpListener,
    redis_storage: RedisStorage<SlackPushEventCallbackJob>,
    settings: Settings,
    notification_service: Arc<RwLock<NotificationService>>,
    task_service: Arc<RwLock<TaskService>>,
    user_service: Arc<RwLock<UserService>>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    auth_token_service: Arc<RwLock<AuthenticationTokenService>>,
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
    let csp_header_value = build_csp_header(&settings);

    // Setup HTTP session + JWT auth
    let session_secret_key = Key::from(settings.application.http_session.secret_key.as_bytes());
    let max_age_days = settings.application.http_session.max_age_days;
    let jwt_signing_keys =
        JWTSigningKeys::load_from_base64_encoded_keys(JWTBase64EncodedSigningKeys {
            secret_key: settings.application.http_session.jwt_secret_key.clone(),
            public_key: settings.application.http_session.jwt_public_key.clone(),
        })?;
    let auth_middleware_settings = {
        AuthenticateMiddlewareSettings {
            jwt_decoding_key: jwt_signing_keys.decoding_key,
            jwt_session_key: Some(JWTSessionKey(JWT_SESSION_KEY.to_string())),
            jwt_authorization_header_prefixes: Some(vec!["Bearer".to_string()]),
            jwt_validator: Validation::new(Algorithm::EdDSA),
        }
    };

    let storage_data = web::Data::new(redis_storage.clone());

    let settings_web_data = web::Data::new(settings);

    info!("Listening on {}", listen_address);

    let server = HttpServer::new(move || {
        info!("Mounting API on {}", api_path);

        // Setup JWT invalidation with no way to send invalidated token for now
        let (_, invalidation_events_stream) = mpsc::channel(100);

        let auth_middleware_factory = AuthenticateMiddlewareFactory::<Claims>::new(
            invalidation_events_stream,
            auth_middleware_settings.clone(),
        );

        let api_scope = web::scope(api_path.trim_end_matches('/'))
            .service(routes::auth::scope())
            .service(routes::integration_connection::scope())
            .service(routes::notification::scope())
            .service(routes::task::scope())
            .service(routes::user::scope())
            .service(routes::webhook::scope())
            .app_data(web::Data::new(notification_service.clone()))
            .app_data(web::Data::new(task_service.clone()))
            .app_data(web::Data::new(user_service.clone()))
            .app_data(web::Data::new(integration_connection_service.clone()))
            .app_data(web::Data::new(auth_token_service.clone()));

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
            .wrap_fn(move |req, srv| {
                let fut = srv.call(req);
                async move {
                    let res = fut.await?;
                    info!(
                        "{} {} {}",
                        res.request().method(),
                        res.request().uri().path(),
                        res.status()
                    );
                    Ok(res)
                }
            })
            .wrap(TracingLogger::<AuthenticatedRootSpanBuilder>::new())
            .wrap(RequestMetrics::default())
            .wrap(middleware::Compress::default())
            .wrap(cors)
            // Cookies are reset when returning an 401 because it can be due to an invalid JWT token
            .wrap(ErrorHandlers::new().handler(StatusCode::UNAUTHORIZED, reset_cookies))
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
            .route(
                "/api/front_config",
                web::get().to(routes::config::front_config),
            )
            .service(
                api_scope.wrap(auth_middleware_factory.clone()).wrap(
                    SessionMiddleware::builder(
                        CookieSessionStore::default(),
                        session_secret_key.clone(),
                    )
                    .session_lifecycle(
                        PersistentSession::default().session_ttl(Duration::days(max_age_days)),
                    )
                    .cookie_secure(true)
                    .cookie_http_only(true)
                    .cookie_same_site(SameSite::Lax)
                    .cookie_content_security(CookieContentSecurity::Signed)
                    .build(),
                ),
            )
            .app_data(settings_web_data.clone())
            .app_data(storage_data.clone());

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

pub async fn run_worker(
    workers_count: Option<usize>,
    redis_storage: RedisStorage<SlackPushEventCallbackJob>,
    notification_service: Arc<RwLock<NotificationService>>,
) -> Monitor<TokioExecutor> {
    let count = workers_count.unwrap_or_else(|| {
        thread::available_parallelism()
            .unwrap_or(NonZeroUsize::new(1).unwrap())
            .get()
    });
    info!("Starting {count} asynchronous Workers");
    Monitor::new()
        .register_with_count(
            count,
            WorkerBuilder::new("slack-push-event-worker")
                .layer(
                    TraceLayer::new()
                        .on_request(DefaultOnRequest::default().level(Level::INFO))
                        .on_response(DefaultOnResponse::default().level(Level::INFO)),
                )
                .with_storage(redis_storage.clone())
                .data(notification_service)
                .build_fn(handle_slack_push_event),
        )
        .on_event(|e| {
            let worker_id = e.id();
            match e.inner() {
                Event::Start => {
                    info!("Worker [{worker_id}] started");
                }
                Event::Error(e) => {
                    error!("Worker [{worker_id}] encountered an error: {e}");
                }

                Event::Exit => {
                    info!("Worker [{worker_id}] exited");
                }
                _ => {}
            }
        })
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
    Arc<RwLock<AuthenticationTokenService>>,
) {
    let repository = Arc::new(Repository::new(pool.clone()));

    let auth_token_service = Arc::new(RwLock::new(AuthenticationTokenService::new(
        repository.clone(),
        settings.application.http_session.clone(),
    )));

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
    // TODO: Add Slack service

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
        auth_token_service,
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

fn reset_cookies<B>(mut res: ServiceResponse<B>) -> ActixResult<ErrorHandlerResponse<B>> {
    res.response_mut().add_cookie(
        &Cookie::build("id", "")
            .path("/")
            .http_only(true)
            .secure(true)
            .same_site(SameSite::Lax)
            .expires(OffsetDateTime::now_utc())
            .max_age(Duration::seconds(0))
            .finish(),
    )?;
    Ok(ErrorHandlerResponse::Response(res.map_into_left_body()))
}
