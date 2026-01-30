#![recursion_limit = "256"]

#[macro_use]
extern crate macro_attr;

#[macro_use]
extern crate enum_derive;

use std::{
    fmt::{Debug, Display},
    net::TcpListener,
    num::NonZeroUsize,
    sync::{Arc, Weak},
    thread,
    time::Duration as StdDuration,
};

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
use anyhow::Context;
use apalis::{
    layers::tracing::{DefaultOnRequest, DefaultOnResponse, OnFailure, TraceLayer},
    prelude::*,
};
use apalis_redis::RedisStorage;
use configuration::AuthenticationSettings;
use csp::{Directive, Source, Sources, CSP};
use futures::channel::mpsc;
use integrations::{api::APIService, google_calendar::GoogleCalendarService, slack::SlackService};
use jobs::UniversalInboxJob;
use jsonwebtoken::{Algorithm, Validation};
use mailer::Mailer;
use sqlx::PgPool;
use tokio::sync::RwLock;
use tracing::{error, event, info, Level, Span};
use tracing_actix_web::TracingLogger;
use utils::cache::Cache;
use webauthn_rs::prelude::*;

use crate::{
    configuration::Settings,
    integrations::{
        github::GithubService, google_drive::GoogleDriveService, google_mail::GoogleMailService,
        linear::LinearService, oauth2::NangoService, todoist::TodoistService,
    },
    jobs::handle_universal_inbox_job,
    observability::AuthenticatedRootSpanBuilder,
    repository::Repository,
    universal_inbox::{
        auth_token::service::AuthenticationTokenService,
        integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService, task::service::TaskService,
        third_party::service::ThirdPartyItemService, user::service::UserService,
        UniversalInboxError,
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
    redis_storage: RedisStorage<UniversalInboxJob>,
    settings: Settings,
    notification_service: Arc<RwLock<NotificationService>>,
    task_service: Arc<RwLock<TaskService>>,
    user_service: Arc<UserService>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    auth_token_service: Arc<RwLock<AuthenticationTokenService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
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
    let cache_data = web::Data::new(
        Cache::new(settings.redis.connection_string())
            .await
            .expect("Failed to create cache"),
    );
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
            .service(routes::third_party::scope())
            .app_data(web::Data::new(notification_service.clone()))
            .app_data(web::Data::new(task_service.clone()))
            .app_data(web::Data::new(user_service.clone()))
            .app_data(web::Data::new(auth_token_service.clone()))
            .app_data(web::Data::new(third_party_item_service.clone()));

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
            .wrap(auth_middleware_factory.clone())
            .wrap(
                SessionMiddleware::builder(
                    CookieSessionStore::default(),
                    session_secret_key.clone(),
                )
                .session_lifecycle(
                    PersistentSession::default().session_ttl(Duration::days(max_age_days)),
                )
                .cookie_same_site(SameSite::Lax)
                .cookie_content_security(CookieContentSecurity::Signed)
                .build(),
            )
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
            .service(api_scope)
            .app_data(settings_web_data.clone())
            .app_data(storage_data.clone())
            .app_data(cache_data.clone())
            .app_data(web::Data::new(integration_connection_service.clone()));

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

#[derive(Clone, Debug)]
struct WorkerOnFailure {}

impl<E: Display + Debug> OnFailure<E> for WorkerOnFailure {
    fn on_failure(&mut self, error: &E, latency: StdDuration, span: &Span) {
        event!(
            parent: span,
            Level::ERROR,
            done_in = format!("{} ms", latency.as_millis()),
            "{:?}", error
        );
    }
}

pub async fn run_worker(
    workers_count: Option<usize>,
    redis_storage: RedisStorage<UniversalInboxJob>,
    notification_service: Arc<RwLock<NotificationService>>,
    task_service: Arc<RwLock<TaskService>>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    slack_service: Arc<SlackService>,
) -> Monitor {
    let count = workers_count.unwrap_or_else(|| {
        thread::available_parallelism()
            .unwrap_or(NonZeroUsize::new(1).unwrap())
            .get()
    });
    info!("Starting {count} asynchronous Workers");
    Monitor::new()
        .register(
            WorkerBuilder::new("universal-inbox-worker")
                .layer(
                    TraceLayer::new()
                        .on_request(DefaultOnRequest::default().level(Level::INFO))
                        .on_response(DefaultOnResponse::default().level(Level::INFO))
                        .on_failure(WorkerOnFailure {}),
                )
                .concurrency(count)
                .data(notification_service)
                .data(task_service)
                .data(integration_connection_service)
                .data(third_party_item_service)
                .data(slack_service)
                .backend(redis_storage.clone())
                .build_fn(handle_universal_inbox_job),
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

#[derive(Debug, Clone, Copy)]
pub enum ExecutionContext {
    Http,
    Worker,
}

#[allow(clippy::too_many_arguments)] // ignore for now, to revisit later
pub async fn build_services(
    pool: Arc<PgPool>,
    settings: &Settings,
    github_address: Option<String>,
    linear_graphql_url: Option<String>,
    google_mail_base_url: Option<String>,
    google_drive_base_url: Option<String>,
    google_calendar_base_url: Option<String>,
    slack_base_url: Option<String>,
    todoist_address: Option<String>,
    nango_service: NangoService,
    mailer: Arc<RwLock<dyn Mailer + Send + Sync>>,
    webauthn: Arc<Webauthn>,
    execution_context: ExecutionContext,
) -> (
    Arc<RwLock<NotificationService>>,
    Arc<RwLock<TaskService>>,
    Arc<UserService>,
    Arc<RwLock<IntegrationConnectionService>>,
    Arc<RwLock<AuthenticationTokenService>>,
    Arc<RwLock<ThirdPartyItemService>>,
    Arc<SlackService>,
) {
    let repository = Arc::new(Repository::new(pool.clone()));

    let auth_token_service = Arc::new(RwLock::new(AuthenticationTokenService::new(
        repository.clone(),
        settings.application.http_session.clone(),
    )));

    let user_service = Arc::new(UserService::new(
        repository.clone(),
        settings.application.clone(),
        mailer.clone(),
        webauthn.clone(),
    ));

    let integration_connection_service = Arc::new(RwLock::new(IntegrationConnectionService::new(
        repository.clone(),
        nango_service,
        settings.nango_provider_keys(),
        settings.required_oauth_scopes(),
        user_service.clone(),
        settings
            .application
            .min_sync_notifications_interval_in_minutes,
        settings.application.min_sync_tasks_interval_in_minutes,
    )));

    let todoist_service = Arc::new(
        TodoistService::new(
            todoist_address,
            integration_connection_service.clone(),
            settings.get_integration_max_retry_duration(execution_context, "todoist"),
        )
        .expect("Failed to create new TodoistService"),
    );
    // tag: New notification integration
    let github_settings = settings
        .integrations
        .get("github")
        .expect("Missing Github settings");
    let github_service = Arc::new(
        GithubService::new(
            github_address,
            github_settings.page_size.unwrap_or(100),
            integration_connection_service.clone(),
            settings.get_integration_max_retry_duration(execution_context, "github"),
        )
        .expect("Failed to create new GithubService"),
    );
    let linear_service = Arc::new(
        LinearService::new(
            linear_graphql_url,
            integration_connection_service.clone(),
            settings.get_integration_max_retry_duration(execution_context, "linear"),
        )
        .expect("Failed to create new LinearService"),
    );

    let google_calendar_service = Arc::new(
        GoogleCalendarService::new(
            google_calendar_base_url,
            Arc::downgrade(&integration_connection_service),
            settings.get_integration_max_retry_duration(execution_context, "google_calendar"),
        )
        .expect("Failed to create new GoogleCalendarService"),
    );

    let google_mail_settings = settings
        .integrations
        .get("google_mail")
        .expect("Missing Google Mail settings");
    let google_mail_service = Arc::new(RwLock::new(
        GoogleMailService::new(
            google_mail_base_url,
            google_mail_settings.page_size.unwrap_or(100),
            Arc::downgrade(&integration_connection_service),
            Weak::new(),
            google_calendar_service.clone(),
            settings.get_integration_max_retry_duration(execution_context, "google_mail"),
        )
        .expect("Failed to create new GoogleMailService"),
    ));

    let google_drive_settings = settings
        .integrations
        .get("google_drive")
        .expect("Missing Google Drive settings");
    let google_drive_service = Arc::new(RwLock::new(
        GoogleDriveService::new(
            google_drive_base_url,
            google_drive_settings.page_size.unwrap_or(100),
            Arc::downgrade(&integration_connection_service),
            Weak::new(), // notification_service - will be set later
            settings.get_integration_max_retry_duration(execution_context, "google_drive"),
        )
        .expect("Failed to create new GoogleDriveService"),
    ));

    let slack_service = Arc::new(SlackService::new(
        slack_base_url,
        integration_connection_service.clone(),
    ));
    let api_service = Arc::new(APIService::new());

    let third_party_item_service = Arc::new(RwLock::new(ThirdPartyItemService::new(
        repository.clone(),
        Weak::new(),
        Weak::new(),
        integration_connection_service.clone(),
        todoist_service.clone(),
        slack_service.clone(),
        linear_service.clone(),
        api_service.clone(),
    )));

    // tag: New notification integration
    let notification_service = Arc::new(RwLock::new(NotificationService::new(
        repository.clone(),
        github_service.clone(),
        linear_service.clone(),
        google_calendar_service.clone(),
        google_drive_service.clone(),
        google_mail_service.clone(),
        slack_service.clone(),
        Weak::new(),
        integration_connection_service.clone(),
        Arc::downgrade(&third_party_item_service),
        user_service.clone(),
        settings
            .application
            .min_sync_notifications_interval_in_minutes,
    )));

    google_mail_service
        .write()
        .await
        .set_notification_service(Arc::downgrade(&notification_service));
    google_drive_service
        .write()
        .await
        .set_notification_service(Arc::downgrade(&notification_service));

    let task_service = Arc::new(RwLock::new(TaskService::new(
        repository,
        todoist_service.clone(),
        linear_service.clone(),
        Arc::downgrade(&notification_service),
        slack_service.clone(),
        integration_connection_service.clone(),
        user_service.clone(),
        Arc::downgrade(&third_party_item_service),
        settings.application.min_sync_tasks_interval_in_minutes,
    )));

    notification_service
        .write()
        .await
        .set_task_service(Arc::downgrade(&task_service));

    third_party_item_service
        .write()
        .await
        .set_task_service(Arc::downgrade(&task_service));

    third_party_item_service
        .write()
        .await
        .set_notification_service(Arc::downgrade(&notification_service));

    (
        notification_service,
        task_service,
        user_service,
        integration_connection_service,
        auth_token_service,
        third_party_item_service,
        slack_service,
    )
}

fn build_csp_header(settings: &Settings) -> String {
    let nango_ws_scheme = if settings.oauth2.nango_base_url.scheme() == "http" {
        "ws"
    } else {
        "wss"
    };
    let nango_base_url = settings.oauth2.nango_base_url.to_string();
    let mut nango_ws_base_url = settings.oauth2.nango_base_url.clone();
    nango_ws_base_url.set_scheme(nango_ws_scheme).unwrap();
    let mut connect_srcs = Sources::new_with(Source::Self_)
        .push(Source::Host(nango_ws_base_url.as_str()))
        .push(Source::Host(&nango_base_url));
    for oidc_issuer_url in settings
        .application
        .security
        .authentication
        .iter()
        .filter_map(|auth| {
            if let AuthenticationSettings::OpenIDConnect(oidc_settings) = auth {
                Some(oidc_settings.oidc_issuer_url.as_str())
            } else {
                None
            }
        })
    {
        connect_srcs.push_borrowed(Source::Host(oidc_issuer_url));
    }
    for url in settings.application.security.csp_extra_connect_src.iter() {
        connect_srcs.push_borrowed(Source::Host(url));
    }

    CSP::new()
        .push(Directive::DefaultSrc(Sources::new_with(Source::Self_)))
        .push(Directive::ScriptSrc(
            Sources::new_with(Source::Self_)
                .push(Source::WasmUnsafeEval)
                .push(Source::UnsafeInline)
                .push(Source::UnsafeEval)
                .push(Source::Host("https://cdn.headwayapp.co")),
        ))
        .push(Directive::StyleSrc(
            Sources::new_with(Source::Self_).push(Source::UnsafeInline),
        ))
        .push(Directive::ObjectSrc(Sources::new()))
        .push(Directive::ConnectSrc(connect_srcs))
        .push(Directive::ImgSrc(
            Sources::new_with(Source::Host("*"))
                .push(Source::Self_)
                .push(Source::Scheme("data")),
        ))
        .push(Directive::WorkerSrc(Sources::new()))
        .push(Directive::FrameSrc(
            Sources::new_with(Source::Self_).push(Source::Host("https://headway-widget.net")),
        ))
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
