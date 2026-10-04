// Shared test-support module included by both the `api` and `browser` test
// binaries. Some helpers (e.g. the billing/Stripe fakes) are exercised by only
// one binary, so each binary sees the others' helpers as unused — expected for
// a shared `common` module.
#![allow(dead_code)]

use std::{net::TcpListener, str::FromStr, sync::Arc};

use apalis_redis::RedisStorage;
use rstest::*;
use sqlx::PgPool;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;
use wiremock::MockServer;

use universal_inbox::{
    billing::{StripePriceId, SubscriptionStatus as DomainStatus},
    user::UserId,
};
use universal_inbox_api::{
    billing::{
        service::{BillingService, RepositoryIntegrationCounter},
        stripe::{
            CheckoutSessionParams, PortalSessionParams, RawSubscription, StripeClient, StripeError,
            StripeEvent, StripeEventKind,
        },
    },
    configuration::{CronSettings, FreePlanSettings, Settings},
    integrations::slack::SlackService,
    jobs::JobStorage,
    observability::{get_subscriber, init_subscriber},
    repository::Repository,
    universal_inbox::{
        auth_token::service::AuthenticationTokenService,
        integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService, oauth2::service::OAuth2Service,
        task::service::TaskService, third_party::service::ThirdPartyItemService,
        user::service::UserService,
    },
    utils::{cache::Cache, passkey::build_webauthn},
};

use crate::common::mailer::MailerStub;

pub mod mailer;
pub mod test_db;

// ---------------------------------------------------------------------------
// rstest fixtures (shared between API and browser tests)
// ---------------------------------------------------------------------------

#[fixture]
#[once]
pub fn tracing_setup(settings: Settings) {
    info!("Setting up tracing");

    let subscriber = get_subscriber(
        &settings.application.observability.logging.log_directive,
        settings.application.observability.logging.format,
    );
    init_subscriber(
        subscriber,
        log::LevelFilter::from_str(
            &settings
                .application
                .observability
                .logging
                .dependencies_log_level,
        )
        .unwrap_or(log::LevelFilter::Error),
    );
    color_backtrace::install();
}

/// Leases a pristine test database. See [`test_db`] for the isolation model.
#[fixture]
pub async fn db_connection(mut settings: Settings) -> test_db::TestDb {
    test_db::acquire(&mut settings.database).await
}

#[fixture]
pub async fn redis_storage(settings: Settings) -> JobStorage {
    let namespace = format!("universal-inbox:jobs:UniversalInboxJob:{}", Uuid::new_v4());
    RedisStorage::new_with_config(
        apalis_redis::connect(settings.redis.connection_string())
            .await
            .expect("Redis storage connection failed"),
        apalis_redis::Config::default().set_namespace(&namespace),
    )
}

#[fixture]
pub fn settings() -> Settings {
    Settings::new_from_file(Some("config/test".to_string()))
        .expect("Cannot load test configuration")
}

// ---------------------------------------------------------------------------
// Mock servers
// ---------------------------------------------------------------------------

pub struct MockServers {
    pub github: MockServer,
    pub linear: MockServer,
    pub google_calendar: MockServer,
    pub google_mail: MockServer,
    pub google_drive: MockServer,
    pub slack: MockServer,
    pub todoist: MockServer,
    pub ticktick: MockServer,
}

impl MockServers {
    pub async fn start() -> Self {
        // tag: New notification integration
        let github = MockServer::start().await;
        let linear = MockServer::start().await;
        let google_calendar = MockServer::start().await;
        let google_mail = MockServer::start().await;
        let google_drive = MockServer::start().await;
        let slack = MockServer::start().await;
        let todoist = MockServer::start().await;
        let ticktick = MockServer::start().await;

        Self {
            github,
            linear,
            google_calendar,
            google_mail,
            google_drive,
            slack,
            todoist,
            ticktick,
        }
    }
}

// ---------------------------------------------------------------------------
// Shared service builder
// ---------------------------------------------------------------------------

pub struct TestServices {
    pub notification_service: Arc<RwLock<NotificationService>>,
    pub task_service: Arc<RwLock<TaskService>>,
    pub user_service: Arc<UserService>,
    pub integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    pub third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    pub slack_service: Arc<SlackService>,
    pub slack_bridge_service:
        Arc<universal_inbox_api::universal_inbox::slack_bridge::service::SlackBridgeService>,
    pub oauth2_service: Arc<OAuth2Service>,
}

/// Point every provider's OAuth grant revocation endpoint (and the Google and
/// Slack token endpoints) at its mock server so disconnect / account deletion / OAuth
/// callbacks never call a real provider.
pub fn with_mocked_oauth_urls(settings: &Settings, mock_servers: &MockServers) -> Settings {
    let mut settings = settings.clone();
    for (name, integration) in settings.integrations.iter_mut() {
        let revocation_url = match name.as_str() {
            "github" => format!(
                "{}/applications/{}/grant",
                mock_servers.github.uri(),
                integration.oauth_client_id
            ),
            "linear" => format!("{}/oauth/revoke", mock_servers.linear.uri()),
            "slack" => format!("{}/auth.revoke", mock_servers.slack.uri()),
            "todoist" => format!("{}/api/v1/revoke", mock_servers.todoist.uri()),
            "ticktick" => format!("{}/oauth/revoke", mock_servers.ticktick.uri()),
            "google_mail" => format!("{}/revoke", mock_servers.google_mail.uri()),
            "google_calendar" => format!("{}/revoke", mock_servers.google_calendar.uri()),
            "google_drive" => format!("{}/revoke", mock_servers.google_drive.uri()),
            _ => continue,
        };
        integration.oauth_revocation_url =
            Some(revocation_url.parse().expect("valid mock revocation URL"));

        // Google code exchanges and Slack token refreshes hit the
        // integration's mock server too.
        let token_url = match name.as_str() {
            "google_mail" => format!("{}/token", mock_servers.google_mail.uri()),
            "google_calendar" => format!("{}/token", mock_servers.google_calendar.uri()),
            "google_drive" => format!("{}/token", mock_servers.google_drive.uri()),
            "slack" => format!("{}/oauth.v2.access", mock_servers.slack.uri()),
            _ => continue,
        };
        integration.oauth_token_url = Some(token_url.parse().expect("valid mock token URL"));
    }
    settings
}

pub async fn build_test_services(
    pool: Arc<PgPool>,
    settings: &Settings,
    mock_servers: &MockServers,
    mailer: Arc<RwLock<dyn universal_inbox_api::mailer::Mailer + Send + Sync>>,
) -> (TestServices, Arc<RwLock<AuthenticationTokenService>>) {
    let webauthn = Arc::new(
        build_webauthn(&settings.application.front_base_url)
            .expect("Failed to build a Webauthn context"),
    );

    let (
        notification_service,
        task_service,
        user_service,
        integration_connection_service,
        auth_token_service,
        third_party_item_service,
        slack_service,
        slack_bridge_service,
        oauth2_service,
        _billing_service,
    ) = universal_inbox_api::build_services(
        pool,
        &with_mocked_oauth_urls(settings, mock_servers),
        Some(mock_servers.github.uri()),
        Some(mock_servers.linear.uri()),
        Some(mock_servers.google_mail.uri()),
        Some(mock_servers.google_drive.uri()),
        Some(mock_servers.google_calendar.uri()),
        Some(mock_servers.slack.uri()),
        Some(mock_servers.todoist.uri()),
        Some(mock_servers.ticktick.uri()),
        mailer,
        webauthn,
        universal_inbox_api::ExecutionContext::Http,
    )
    .await;

    let services = TestServices {
        notification_service,
        task_service,
        user_service,
        integration_connection_service,
        third_party_item_service,
        slack_service,
        slack_bridge_service,
        oauth2_service,
    };

    (services, auth_token_service)
}

// ---------------------------------------------------------------------------
// Server & worker spawning
// ---------------------------------------------------------------------------

pub async fn spawn_test_server(
    listener: TcpListener,
    redis_storage: JobStorage,
    settings: Settings,
    services: &TestServices,
    auth_token_service: Arc<RwLock<AuthenticationTokenService>>,
) {
    let server = universal_inbox_api::run_server(
        listener,
        redis_storage,
        settings,
        services.notification_service.clone(),
        services.task_service.clone(),
        services.user_service.clone(),
        services.integration_connection_service.clone(),
        auth_token_service,
        services.third_party_item_service.clone(),
        services.slack_bridge_service.clone(),
        services.oauth2_service.clone(),
        None, // billing service: tests opt in explicitly when they need it
    )
    .await
    .expect("Failed to bind address");

    tokio::spawn(server);
}

pub async fn spawn_test_worker(
    redis_storage: JobStorage,
    cron_settings: CronSettings,
    cache: Cache,
    services: &TestServices,
) {
    let worker = universal_inbox_api::run_worker(
        Some(1),
        redis_storage,
        cron_settings,
        cache,
        services.notification_service.clone(),
        services.task_service.clone(),
        services.integration_connection_service.clone(),
        services.third_party_item_service.clone(),
        services.slack_service.clone(),
    )
    .await;

    tokio::spawn(worker.run());
}

// ---------------------------------------------------------------------------
// Common test environment setup
// ---------------------------------------------------------------------------

/// Sets up the common test environment: rustls, listener, cache, mock servers.
/// Returns (listener, port, cache, mock_servers).
pub async fn setup_test_env(settings: &Settings) -> (TcpListener, u16, Cache, MockServers) {
    // Use `let _ =` because `install_default` can only succeed once per process.
    // Subsequent calls (from other tests in the same binary) return Err, which is harmless.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind random port");
    let port = listener.local_addr().unwrap().port();

    let cache = Cache::new(settings.redis.connection_string())
        .await
        .expect("Failed to create cache");
    Cache::set_namespace(Uuid::new_v4().to_string()).await;

    let mock_servers = MockServers::start().await;

    (listener, port, cache, mock_servers)
}

/// Builds services, spawns server and worker. Returns (TestServices, mailer_stub, redis_storage).
pub async fn build_and_spawn(
    listener: TcpListener,
    pool: Arc<PgPool>,
    settings: Settings,
    mock_servers: &MockServers,
    redis_storage: JobStorage,
) -> (TestServices, Arc<RwLock<MailerStub>>, JobStorage) {
    let mailer_stub = Arc::new(RwLock::new(MailerStub::new()));
    let (services, auth_token_service) =
        build_test_services(pool, &settings, mock_servers, mailer_stub.clone()).await;

    let cron_settings = settings.application.cron.clone();
    let cache = Cache::new(settings.redis.connection_string())
        .await
        .expect("Failed to create cache");
    spawn_test_server(
        listener,
        redis_storage.clone(),
        settings,
        &services,
        auth_token_service,
    )
    .await;

    spawn_test_worker(redis_storage.clone(), cron_settings, cache, &services).await;

    (services, mailer_stub, redis_storage)
}

// ---------------------------------------------------------------------------
// Billing test harness — fake Stripe client + injectable BillingService
// ---------------------------------------------------------------------------

/// Query marker distinguishing the fake Checkout / Portal destinations.
///
/// The fake Stripe URLs point back at the test server's own unauthenticated
/// `/ping`, because the browser has to actually *land* on them for the
/// redirect to be observable: a top-level navigation to an off-origin sentinel
/// host never resolves (and `page.route` does not reliably intercept a
/// main-frame navigation here), and Chrome flatly blocks top-level `data:`
/// navigation. `/ping` is real, resolvable, and tiny. It must stay outside the
/// API scope: API responses are forced to `Content-Disposition: attachment`,
/// so the browser would download `/api/ping` instead of navigating to it.
pub const FAKE_CHECKOUT_MARKER: &str = "stripe_checkout=1";
pub const FAKE_PORTAL_MARKER: &str = "stripe_portal=1";
pub const FAKE_STRIPE_CUSTOMER_ID: &str = "cus_fake_test";

/// The URL [`FakeStripeClient`] redirects Checkout to, for a given app base URL.
pub fn fake_checkout_url(app_base_url: &url::Url) -> url::Url {
    fake_stripe_url(app_base_url, FAKE_CHECKOUT_MARKER)
}

/// The URL [`FakeStripeClient`] redirects the billing Portal to.
pub fn fake_portal_url(app_base_url: &url::Url) -> url::Url {
    fake_stripe_url(app_base_url, FAKE_PORTAL_MARKER)
}

fn fake_stripe_url(app_base_url: &url::Url, marker: &str) -> url::Url {
    let mut url = app_base_url
        .join("/ping")
        .expect("static fake Stripe path joins");
    url.set_query(Some(marker));
    url
}

/// Minimal JSON envelope a test posts to `/api/billing/stripe/webhook`. The
/// fake client parses it directly (no real signature) so tests can drive
/// webhook delivery over HTTP exactly as Stripe would.
#[derive(serde::Deserialize)]
struct TestWebhookEnvelope {
    id: String,
    #[serde(rename = "type")]
    type_: String,
    created: i64,
    customer: Option<String>,
    subscription: Option<String>,
    user_id: Option<uuid::Uuid>,
}

/// In-process `StripeClient` that never touches the network. Lets the running
/// HTTP server expose the real `/api/billing/*` routes without a Stripe account.
pub struct FakeStripeClient {
    checkout_url: url::Url,
    portal_url: url::Url,
}

impl FakeStripeClient {
    /// `app_base_url` is the running test server's base URL; the returned
    /// Checkout/Portal URLs point back at it so the browser can really land on
    /// them (see [`fake_checkout_url`]).
    pub fn new(app_base_url: &url::Url) -> Self {
        Self {
            checkout_url: fake_checkout_url(app_base_url),
            portal_url: fake_portal_url(app_base_url),
        }
    }
}

#[async_trait::async_trait]
impl StripeClient for FakeStripeClient {
    async fn create_customer(
        &self,
        _user_id: UserId,
        _email: Option<&str>,
    ) -> Result<String, StripeError> {
        Ok(FAKE_STRIPE_CUSTOMER_ID.to_string())
    }

    async fn create_checkout_session(
        &self,
        _params: CheckoutSessionParams,
    ) -> Result<url::Url, StripeError> {
        Ok(self.checkout_url.clone())
    }

    async fn create_portal_session(
        &self,
        _params: PortalSessionParams,
    ) -> Result<url::Url, StripeError> {
        Ok(self.portal_url.clone())
    }

    async fn fetch_subscription(
        &self,
        subscription_id: &str,
    ) -> Result<RawSubscription, StripeError> {
        Ok(RawSubscription {
            subscription_id: subscription_id.to_string(),
            customer_id: FAKE_STRIPE_CUSTOMER_ID.to_string(),
            price_id: Some("price_test".to_string()),
            status: DomainStatus::Active,
            current_period_start: None,
            current_period_end: None,
            cancel_at_period_end: false,
            canceled_at: None,
        })
    }

    async fn list_customer_subscriptions(
        &self,
        _customer_id: &str,
    ) -> Result<Vec<RawSubscription>, StripeError> {
        // Nothing to adopt: these tests drive billing state through webhooks.
        Ok(vec![])
    }

    async fn cancel_subscription(
        &self,
        subscription_id: &str,
    ) -> Result<RawSubscription, StripeError> {
        Ok(RawSubscription {
            subscription_id: subscription_id.to_string(),
            customer_id: FAKE_STRIPE_CUSTOMER_ID.to_string(),
            price_id: Some("price_test".to_string()),
            status: DomainStatus::Canceled,
            current_period_start: None,
            current_period_end: None,
            cancel_at_period_end: false,
            canceled_at: None,
        })
    }

    async fn clear_customer_user_id(&self, _customer_id: &str) -> Result<(), StripeError> {
        Ok(())
    }

    fn verify_webhook_signature(
        &self,
        payload: &str,
        _signature_header: &str,
    ) -> Result<StripeEvent, StripeError> {
        let env: TestWebhookEnvelope = serde_json::from_str(payload)
            .map_err(|err| StripeError::MalformedWebhookPayload(err.to_string()))?;
        let created = chrono::DateTime::from_timestamp(env.created, 0).ok_or_else(|| {
            StripeError::MalformedWebhookPayload(format!("bad created timestamp {}", env.created))
        })?;
        let kind = match env.type_.as_str() {
            "checkout.session.completed" => StripeEventKind::CheckoutSessionCompleted {
                customer_id: env.customer.clone(),
                subscription_id: env.subscription.clone(),
                user_id_metadata: env.user_id.map(UserId),
            },
            _ => StripeEventKind::Other,
        };
        Ok(StripeEvent {
            id: env.id,
            type_: env.type_,
            created,
            kind,
        })
    }
}

/// Build a `BillingService` wired to [`FakeStripeClient`] so the running server
/// serves the billing routes against the test database without real Stripe.
pub fn build_fake_billing_service(
    repository: Arc<Repository>,
    app_base_url: &url::Url,
) -> Arc<BillingService> {
    let counter = Arc::new(RepositoryIntegrationCounter {
        repository: repository.clone(),
    });
    Arc::new(BillingService::new(
        repository,
        Arc::new(FakeStripeClient::new(app_base_url)),
        &FreePlanSettings::default(),
        StripePriceId("price_test".to_string()),
        counter,
    ))
}

/// Like [`spawn_test_server`] but passes a `BillingService` so `/api/billing/*`
/// is mounted (the route gate keys on `Some(billing_service)`).
pub async fn spawn_test_server_with_billing(
    listener: TcpListener,
    redis_storage: JobStorage,
    settings: Settings,
    services: &TestServices,
    auth_token_service: Arc<RwLock<AuthenticationTokenService>>,
    billing_service: Option<Arc<BillingService>>,
) {
    let server = universal_inbox_api::run_server(
        listener,
        redis_storage,
        settings,
        services.notification_service.clone(),
        services.task_service.clone(),
        services.user_service.clone(),
        services.integration_connection_service.clone(),
        auth_token_service,
        services.third_party_item_service.clone(),
        services.slack_bridge_service.clone(),
        services.oauth2_service.clone(),
        billing_service,
    )
    .await
    .expect("Failed to bind address");

    tokio::spawn(server);
}

/// Like [`build_and_spawn`] but with billing enabled via `billing_service`.
pub async fn build_and_spawn_with_billing(
    listener: TcpListener,
    pool: Arc<PgPool>,
    settings: Settings,
    mock_servers: &MockServers,
    redis_storage: JobStorage,
    billing_service: Option<Arc<BillingService>>,
) -> (TestServices, Arc<RwLock<MailerStub>>, JobStorage) {
    let mailer_stub = Arc::new(RwLock::new(MailerStub::new()));
    let (services, auth_token_service) =
        build_test_services(pool, &settings, mock_servers, mailer_stub.clone()).await;

    let cron_settings = settings.application.cron.clone();
    let cache = Cache::new(settings.redis.connection_string())
        .await
        .expect("Failed to create cache");

    spawn_test_server_with_billing(
        listener,
        redis_storage.clone(),
        settings,
        &services,
        auth_token_service,
        billing_service,
    )
    .await;

    spawn_test_worker(redis_storage.clone(), cron_settings, cache, &services).await;

    (services, mailer_stub, redis_storage)
}
