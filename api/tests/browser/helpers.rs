use std::{collections::HashMap, env, sync::Arc, time::Duration};

use apalis_redis::RedisStorage;
use rstest::*;
use sqlx::PgPool;
use tokio::sync::{OnceCell, RwLock};
use tracing::info;
use wiremock::MockServer;

use playwright_rs::{Browser, BrowserContext, LaunchOptions, Page, Playwright, expect};

/// Timeout for Playwright expect assertions.
/// Debug WASM binaries (~74 MB) take significant time to download and initialize,
/// especially on resource-constrained CI runners.
pub const EXPECT_TIMEOUT: Duration = Duration::from_secs(60);

use universal_inbox_api::{
    commands::generate::generate_testing_user,
    configuration::{AuthenticationSettings, LocalAuthenticationSettings, Settings},
    jobs::UniversalInboxJob,
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService, task::service::TaskService,
        third_party::service::ThirdPartyItemService, user::service::UserService,
    },
    utils::cache::Cache,
};

use crate::common::{build_and_spawn, setup_test_env};

// Re-export shared fixtures so rstest can resolve them by name in this module's fixtures
pub use crate::common::{db_connection, redis_storage, settings, tracing_setup};

pub const DEFAULT_PASSWORD: &str = "test123456";

pub struct BrowserTestedApp {
    pub app_url: String,
    pub user_service: Arc<UserService>,
    pub task_service: Arc<RwLock<TaskService>>,
    pub notification_service: Arc<RwLock<NotificationService>>,
    pub integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    pub third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    pub settings: Settings,
    pub _cache: Cache,
    // Keep mock servers alive for the duration of the test
    pub _github_mock_server: MockServer,
    pub _linear_mock_server: MockServer,
    pub _google_calendar_mock_server: MockServer,
    pub _google_mail_mock_server: MockServer,
    pub _google_drive_mock_server: MockServer,
    pub _slack_mock_server: MockServer,
    pub _todoist_mock_server: MockServer,
    pub _nango_mock_server: MockServer,
}

impl Drop for BrowserTestedApp {
    fn drop(&mut self) {
        let cache = self._cache.clone();
        tokio::spawn(async move {
            let _ = cache.clear(&None).await;
        });
    }
}

#[fixture]
pub async fn browser_tested_app(
    mut settings: Settings,
    #[allow(unused, clippy::let_unit_value)] tracing_setup: (),
    #[future] db_connection: Arc<PgPool>,
    #[future] redis_storage: RedisStorage<UniversalInboxJob>,
) -> BrowserTestedApp {
    info!("Setting up browser test server");

    let (listener, port, cache, mock_servers) = setup_test_env(&settings).await;

    // Configure local auth (password-based)
    settings.application.security.authentication =
        vec![AuthenticationSettings::Local(LocalAuthenticationSettings {
            argon2_algorithm: argon2::Algorithm::Argon2id,
            argon2_version: argon2::Version::V0x13,
            argon2_memory_size: 20000,
            argon2_iterations: 2,
            argon2_parallelism: 1,
        })];
    settings.application.security.email_domain_blacklist = HashMap::new();

    // Configure static file serving for the WASM frontend
    // Use `localhost` (not 127.0.0.1) so that Url::domain() returns Some("localhost"),
    // which is required by the Webauthn context builder.
    settings.application.front_base_url = format!("http://localhost:{port}").parse().unwrap();
    settings.application.static_path = Some("".to_string());
    settings.application.static_dir = Some(format!(
        "{}/../web/public",
        env::var("CARGO_MANIFEST_DIR").unwrap()
    ));

    let pool: Arc<PgPool> = db_connection.await;
    let redis_storage = redis_storage.await;

    let (services, _mailer_stub, _redis_storage) = build_and_spawn(
        listener,
        pool,
        settings.clone(),
        &mock_servers,
        redis_storage,
    )
    .await;

    let app_url = format!("http://localhost:{port}");

    BrowserTestedApp {
        app_url,
        user_service: services.user_service,
        task_service: services.task_service,
        notification_service: services.notification_service,
        integration_connection_service: services.integration_connection_service,
        third_party_item_service: services.third_party_item_service,
        settings,
        _cache: cache,
        _github_mock_server: mock_servers.github,
        _linear_mock_server: mock_servers.linear,
        _google_calendar_mock_server: mock_servers.google_calendar,
        _google_mail_mock_server: mock_servers.google_mail,
        _google_drive_mock_server: mock_servers.google_drive,
        _slack_mock_server: mock_servers.slack,
        _todoist_mock_server: mock_servers.todoist,
        _nango_mock_server: mock_servers.nango,
    }
}

/// Shared across all tests to avoid re-launching Playwright + Chromium per test (~5-10s each).
/// Test isolation is preserved: each test gets a fresh `BrowserContext` + `Page`.
static SHARED_BROWSER: OnceCell<(Playwright, Browser)> = OnceCell::const_new();

/// Launch a headless Chromium browser and return a new page.
///
/// External requests (e.g., CDN scripts, analytics) are blocked so they don't
/// stall page initialization in the isolated test environment.
pub async fn launch_browser() -> (BrowserContext, Page) {
    let (_playwright, browser) = SHARED_BROWSER
        .get_or_init(|| async {
            let playwright = Playwright::launch()
                .await
                .expect("Failed to launch Playwright");
            // Disable Chromium sandbox on CI (Linux containers lack required kernel features)
            let launch_options = LaunchOptions::default().chromium_sandbox(false);
            let browser = playwright
                .chromium()
                .launch_with_options(launch_options)
                .await
                .expect("Failed to launch Chromium");
            (playwright, browser)
        })
        .await;

    let context = browser
        .new_context()
        .await
        .expect("Failed to create browser context");
    let page = context.new_page().await.expect("Failed to create page");

    // Block external network requests that may hang in isolated test environments
    page.route("**/*headwayapp.co*", |route| async move {
        route.abort(None).await
    })
    .await
    .expect("Failed to set up route interception for headwayapp.co");
    page.route("**/*cdn.*", |route| async move { route.abort(None).await })
        .await
        .expect("Failed to set up route interception for cdn");

    (context, page)
}

/// Generate a test user with sample data and return the email address.
pub async fn generate_test_user(app: &BrowserTestedApp) -> String {
    generate_testing_user(
        app.user_service.clone(),
        app.integration_connection_service.clone(),
        app.notification_service.clone(),
        app.task_service.clone(),
        app.third_party_item_service.clone(),
        app.settings.clone(),
    )
    .await
    .expect("Failed to generate test user")
}

/// Log in a user via the browser by filling the login form.
pub async fn login(page: &Page, app_url: &str, email: &str) {
    page.goto(&format!("{app_url}/login"), None)
        .await
        .expect("Failed to navigate to login page");

    // Wait for the WASM app to load using auto-retry expect with extended timeout
    // for the debug WASM binary to download and initialize.
    let email_input = page.locator("input[name='email']").await;
    expect(email_input.clone())
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Email input not visible on login page");

    email_input
        .fill(email, None)
        .await
        .expect("Failed to fill email");

    let password_input = page.locator("input[name='password']").await;
    password_input
        .fill(DEFAULT_PASSWORD, None)
        .await
        .expect("Failed to fill password");

    let submit_button = page.locator("button[type='submit']").await;
    submit_button
        .click(None)
        .await
        .expect("Failed to click submit");

    // Wait for redirect away from login by checking that the notifications page is visible
    let notifications_page = page.locator("#notifications-page").await;
    expect(notifications_page)
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Notifications page not visible after login");
}

/// Register a new user via the browser by filling the signup form.
pub async fn register(page: &Page, app_url: &str, email: &str) {
    page.goto(&format!("{app_url}/signup"), None)
        .await
        .expect("Failed to navigate to signup page");

    let email_input = page.locator("input[name='email']").await;
    expect(email_input.clone())
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Email input not visible on signup page");

    email_input
        .fill(email, None)
        .await
        .expect("Failed to fill email");

    let password_input = page.locator("input[name='password']").await;
    password_input
        .fill(DEFAULT_PASSWORD, None)
        .await
        .expect("Failed to fill password");

    let submit_button = page.locator("button[type='submit']").await;
    submit_button
        .click(None)
        .await
        .expect("Failed to click submit");

    // Wait for redirect away from signup — the notifications page should appear
    // after auto-login.  We must NOT wait on `input[name='email']` because that
    // element already exists on the signup page and would match immediately.
    let notifications_page = page.locator("#notifications-page").await;
    expect(notifications_page)
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Notifications page not visible after signup (expected redirect)");
}

/// Wait until at least one notification row is visible in the DOM.
/// After login the notification list may still be loading from the API.
pub async fn wait_for_notification_rows(page: &Page) {
    let first_row = page.locator("#notifications-list table tr.row-hover").await;
    expect(first_row.first())
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Expected at least one notification row to be visible");
}

/// Navigate within the SPA by clicking an `<a>` link and assert an element is visible.
///
/// Using SPA link clicks avoids a full page reload which would re-download
/// the ~74 MB debug WASM binary (adding ~60 s to each navigation).
pub async fn navigate_and_assert(page: &Page, path: &str, expected_selector: &str) {
    let link = page.locator(&format!("a[href='{path}']")).await;
    link.first()
        .click(None)
        .await
        .unwrap_or_else(|_| panic!("Failed to click link to {path}"));
    let element = page.locator(expected_selector).await;
    expect(element.first())
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .unwrap_or_else(|_| panic!("Expected element '{expected_selector}' not visible on {path}"));
}
