#![recursion_limit = "256"]

use std::{
    collections::HashMap,
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
    SessionMiddleware,
    config::{CookieContentSecurity, PersistentSession},
    storage::CookieSessionStore,
};
use actix_web::{
    App, HttpServer, Result as ActixResult,
    cookie::{
        Cookie, Key, SameSite,
        time::{Duration, OffsetDateTime},
    },
    dev::{Server, Service, ServiceResponse},
    http::{self, header},
    middleware::{self, ErrorHandlerResponse, ErrorHandlers},
    web,
};
use actix_web_lab::web::spa;
use anyhow::Context;
use apalis::{
    layers::tracing::{DefaultOnRequest, DefaultOnResponse, OnFailure, TraceLayer},
    prelude::*,
};
use apalis_redis::RedisStorage;
use configuration::AuthenticationSettings;
use csp::{CSP, Directive, Source, Sources};
use futures::channel::mpsc;
use integrations::{api::APIService, google_calendar::GoogleCalendarService, slack::SlackService};
use jobs::UniversalInboxJob;
use jsonwebtoken::{Algorithm, Validation};
use mailer::Mailer;
use sqlx::PgPool;
use tokio::sync::RwLock;
use tracing::{Level, Span, error, event, info};
use tracing_actix_web::TracingLogger;
use utils::cache::Cache;
use webauthn_rs::prelude::*;

use crate::{
    configuration::Settings,
    integrations::{
        github::{GithubService, oauth::GithubOAuth2Provider},
        google_drive::GoogleDriveService,
        google_mail::GoogleMailService,
        google_oauth::GoogleOAuth2Provider,
        linear::{LinearService, oauth::LinearOAuth2Provider},
        oauth2::{
            NangoService,
            provider::{OAuth2FlowService, OAuth2Provider},
        },
        slack_oauth::SlackOAuth2Provider,
        todoist::TodoistService,
        todoist_oauth::TodoistOAuth2Provider,
    },
    jobs::handle_universal_inbox_job,
    observability::AuthenticatedRootSpanBuilder,
    repository::Repository,
    universal_inbox::{
        UniversalInboxError, auth_token::service::AuthenticationTokenService,
        integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService, oauth2::service::OAuth2Service,
        slack_bridge::service::SlackBridgeService, task::service::TaskService,
        third_party::service::ThirdPartyItemService, user::service::UserService,
    },
    utils::{
        crypto::TokenEncryptionKey,
        jwt::{Claims, JWT_SESSION_KEY, JWTBase64EncodedSigningKeys, JWTSigningKeys},
    },
};

use secrecy::SecretBox;

pub mod commands;
pub mod configuration;
pub mod integrations;
pub mod jobs;
pub mod mailer;
pub mod mcp;
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
    slack_bridge_service: Arc<SlackBridgeService>,
    oauth2_service: Arc<OAuth2Service>,
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
    let api_version = settings.application.version.clone();

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
            jwt_validator: {
                let mut validation = Validation::new(Algorithm::EdDSA);
                // Disable aud validation at the global JWT level because
                // OAuth2 tokens carry an `aud` claim that the MCP
                // RequireAuthenticated middleware validates against the
                // resource URL — enabling it here (with no expected
                // audience configured) would reject them before they
                // reach that middleware.  Session tokens omit `aud`.
                //
                // Trade-off: an OAuth2 MCP token could technically be
                // used on non-MCP API routes.  The MCP middleware's
                // Bearer-only + audience checks keep MCP routes safe;
                // a future improvement could add a similar guard on the
                // API scope to reject tokens that carry an `aud` claim.
                validation.validate_aud = false;
                validation
            },
        }
    };

    let storage_data = web::Data::new(redis_storage.clone());
    let cache_data = web::Data::new(
        Cache::new(settings.redis.connection_string())
            .await
            .expect("Failed to create cache"),
    );
    let mcp_extra_allowed_origins = settings
        .application
        .security
        .mcp_extra_allowed_origins
        .clone();
    let settings_web_data = web::Data::new(settings);

    info!("Listening on {}", listen_address);

    // Build the MCP service and rate limiter once so they are shared across
    // all Actix-web worker threads (each worker clones these Arc-backed values).
    let mcp_http_service = mcp::build_http_service(
        notification_service.clone(),
        task_service.clone(),
        redis_storage.clone(),
    );
    let oauth2_rate_limiter = routes::oauth2::build_rate_limiter();
    let mcp_rate_limiter = mcp::build_rate_limiter();

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
            .service(routes::oauth::authorize_scope())
            .service(routes::oauth2::scope(oauth2_rate_limiter.clone()))
            .service(routes::notification::scope())
            .service(routes::task::scope())
            .service(routes::user::scope())
            .service(routes::webhook::scope())
            .service(routes::third_party::scope())
            .service(routes::slack_bridge::scope())
            .service(mcp::scope(
                mcp_http_service.clone(),
                mcp_rate_limiter.clone(),
                format!("{front_base_url}{api_path}mcp"),
                mcp_extra_allowed_origins.clone(),
            ))
            .app_data(web::Data::new(notification_service.clone()))
            .app_data(web::Data::new(task_service.clone()))
            .app_data(web::Data::new(user_service.clone()))
            .app_data(web::Data::new(auth_token_service.clone()))
            .app_data(web::Data::new(third_party_item_service.clone()))
            .app_data(web::Data::new(slack_bridge_service.clone()))
            .app_data(web::Data::new(oauth2_service.clone()));

        let api_path_for_cors = api_path.clone();
        let mcp_extra_origins_for_cors = mcp_extra_allowed_origins.clone();
        let cors = Cors::default()
            .allowed_origin(&front_base_url)
            .allowed_origin_fn(move |origin, req_head| {
                // Allow configured MCP extra origins (e.g. MCP inspector)
                let origin_str = origin.to_str().unwrap_or("");
                if mcp_extra_origins_for_cors.iter().any(|o| o == origin_str) {
                    return true;
                }
                // Allow any origin for MCP and OAuth2 endpoints:
                // these use Bearer token auth (no CSRF risk via cookies).
                let path = req_head.uri.path();
                let mcp_prefix = format!("{}/mcp", api_path_for_cors.trim_end_matches('/'));
                path.starts_with(&mcp_prefix)
                    || path.starts_with("/.well-known/oauth-")
                    || path.starts_with(&format!(
                        "{}/oauth2",
                        api_path_for_cors.trim_end_matches('/')
                    ))
            })
            .allowed_methods(vec!["GET", "POST", "PATCH", "DELETE", "PUT"])
            .allowed_headers(vec![
                http::header::AUTHORIZATION,
                http::header::COOKIE,
                http::header::CONTENT_TYPE,
                http::header::ACCEPT,
                http::header::HeaderName::from_static("mcp-session-id"),
                http::header::HeaderName::from_static("mcp-protocol-version"),
            ])
            .expose_headers(vec![
                http::header::HeaderName::from_static("x-app-version"),
                http::header::HeaderName::from_static("mcp-session-id"),
            ])
            .supports_credentials()
            .max_age(3600);

        let csp_header_value = csp_header_value.clone();
        let api_version = api_version.clone();
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
                let api_version = api_version.clone();
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
                    if let Some(ref version) = api_version {
                        res.headers_mut().insert(
                            header::HeaderName::from_static("x-app-version"),
                            header::HeaderValue::from_str(version).unwrap(),
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
            .route(
                "/api/oauth/callback",
                web::get().to(routes::oauth::oauth_callback),
            )
            .route(
                "/.well-known/oauth-protected-resource",
                web::get().to(routes::well_known::protected_resource_metadata),
            )
            .route(
                &format!("/.well-known/oauth-protected-resource{}mcp", api_path),
                web::get().to(routes::well_known::protected_resource_metadata),
            )
            .route(
                "/.well-known/oauth-authorization-server",
                web::get().to(routes::well_known::authorization_server_metadata),
            )
            .service(api_scope)
            .app_data(web::Data::new(notification_service.clone()))
            .app_data(web::Data::new(task_service.clone()))
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
    Arc<SlackBridgeService>,
    Arc<OAuth2Service>,
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

    // Build the map of internal OAuth2 providers (integrations not using Nango)
    use ::universal_inbox::integration_connection::provider::IntegrationProviderKind;
    let mut oauth2_providers: HashMap<IntegrationProviderKind, Arc<dyn OAuth2Provider>> =
        HashMap::new();
    if let Some(linear_settings) = settings.integrations.get("linear")
        && let (Some(client_id), Some(client_secret)) = (
            linear_settings.oauth_client_id.as_ref(),
            linear_settings.oauth_client_secret.as_ref(),
        )
    {
        oauth2_providers.insert(
            IntegrationProviderKind::Linear,
            Arc::new(LinearOAuth2Provider::new(
                client_id.clone(),
                SecretBox::new(Box::new(client_secret.clone())),
                linear_settings.required_oauth_scopes.clone(),
            )),
        );
    }
    if let Some(github_settings) = settings.integrations.get("github")
        && let (Some(client_id), Some(client_secret)) = (
            github_settings.oauth_client_id.as_ref(),
            github_settings.oauth_client_secret.as_ref(),
        )
    {
        oauth2_providers.insert(
            IntegrationProviderKind::Github,
            Arc::new(GithubOAuth2Provider::new(
                client_id.clone(),
                SecretBox::new(Box::new(client_secret.clone())),
                github_settings.required_oauth_scopes.clone(),
            )),
        );
    }
    if let Some(slack_settings) = settings.integrations.get("slack")
        && let (Some(client_id), Some(client_secret)) = (
            slack_settings.oauth_client_id.as_ref(),
            slack_settings.oauth_client_secret.as_ref(),
        )
    {
        oauth2_providers.insert(
            IntegrationProviderKind::Slack,
            Arc::new(SlackOAuth2Provider::new(
                client_id.clone(),
                SecretBox::new(Box::new(client_secret.clone())),
                slack_settings.required_oauth_scopes.clone(),
            )),
        );
    }
    if let Some(todoist_settings) = settings.integrations.get("todoist")
        && let (Some(client_id), Some(client_secret)) = (
            todoist_settings.oauth_client_id.as_ref(),
            todoist_settings.oauth_client_secret.as_ref(),
        )
    {
        oauth2_providers.insert(
            IntegrationProviderKind::Todoist,
            Arc::new(TodoistOAuth2Provider::new(
                client_id.clone(),
                SecretBox::new(Box::new(client_secret.clone())),
                todoist_settings.required_oauth_scopes.clone(),
            )),
        );
    }
    for (settings_key, provider_kind) in [
        ("google_mail", IntegrationProviderKind::GoogleMail),
        ("google_calendar", IntegrationProviderKind::GoogleCalendar),
        ("google_drive", IntegrationProviderKind::GoogleDrive),
    ] {
        if let Some(google_settings) = settings.integrations.get(settings_key)
            && let (Some(client_id), Some(client_secret)) = (
                google_settings.oauth_client_id.as_ref(),
                google_settings.oauth_client_secret.as_ref(),
            )
        {
            oauth2_providers.insert(
                provider_kind,
                Arc::new(GoogleOAuth2Provider::new(
                    provider_kind,
                    client_id.clone(),
                    SecretBox::new(Box::new(client_secret.clone())),
                    google_settings.required_oauth_scopes.clone(),
                )),
            );
        }
    }

    let token_encryption_key = settings
        .oauth2
        .token_encryption_key
        .as_ref()
        .map(|hex| TokenEncryptionKey::from_hex(hex).map(|k| SecretBox::new(Box::new(k))))
        .transpose()
        .expect("Invalid token encryption key");

    let redirect_uri = settings
        .application
        .get_oauth_redirect_url()
        .expect("Failed to compute OAuth redirect URL");
    let oauth2_flow_service =
        Some(OAuth2FlowService::new(redirect_uri).expect("Failed to create OAuth2FlowService"));

    let integration_connection_service = Arc::new(RwLock::new(IntegrationConnectionService::new(
        repository.clone(),
        nango_service,
        settings.nango_provider_keys(),
        settings.required_oauth_scopes(),
        oauth2_providers,
        oauth2_flow_service,
        token_encryption_key,
        user_service.clone(),
        settings
            .application
            .min_sync_notifications_interval_in_minutes,
        settings.application.min_sync_tasks_interval_in_minutes,
        settings.application.sync_backoff_base_delay_in_seconds,
        settings.application.sync_backoff_max_delay_in_seconds,
        settings.application.sync_failure_window_in_hours,
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

    let slack_bridge_service = Arc::new(SlackBridgeService::new(repository.clone()));
    let slack_service = Arc::new(SlackService::new(
        slack_base_url,
        repository.clone(),
        integration_connection_service.clone(),
        slack_bridge_service.clone(),
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
        repository.clone(),
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

    let resource_url = format!(
        "{}{}mcp",
        settings
            .application
            .front_base_url
            .as_str()
            .trim_end_matches('/'),
        settings.application.api_path
    );
    let oauth2_service = Arc::new(OAuth2Service::new(
        repository,
        settings.application.http_session.jwt_secret_key.clone(),
        settings.application.http_session.jwt_public_key.clone(),
        resource_url,
    ));

    (
        notification_service,
        task_service,
        user_service,
        integration_connection_service,
        auth_token_service,
        third_party_item_service,
        slack_service,
        slack_bridge_service,
        oauth2_service,
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
    connect_srcs = connect_srcs
        .push(Source::Host("https://client.crisp.chat"))
        .push(Source::Host("wss://client.relay.crisp.chat"));

    CSP::new()
        .push(Directive::DefaultSrc(Sources::new_with(Source::Self_)))
        .push(Directive::ScriptSrc(
            Sources::new_with(Source::Self_)
                .push(Source::WasmUnsafeEval)
                .push(Source::UnsafeInline)
                .push(Source::UnsafeEval)
                .push(Source::Host("https://client.crisp.chat"))
                .push(Source::Host("https://cdn.headwayapp.co")),
        ))
        .push(Directive::StyleSrc(
            Sources::new_with(Source::Self_)
                .push(Source::UnsafeInline)
                .push(Source::Host("https://client.crisp.chat")),
        ))
        .push(Directive::ObjectSrc(Sources::new()))
        .push(Directive::ConnectSrc(connect_srcs))
        .push(Directive::ImgSrc(
            Sources::new_with(Source::Host("*"))
                .push(Source::Self_)
                .push(Source::Scheme("data")),
        ))
        .push(Directive::FontSrc(
            Sources::new_with(Source::Self_).push(Source::Host("https://client.crisp.chat")),
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
