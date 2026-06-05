use std::{collections::HashMap, env, sync::Arc, time::Duration};

use apalis_redis::RedisStorage;
use email_address::EmailAddress;
use rstest::*;
use sqlx::PgPool;
use tokio::sync::{OnceCell, RwLock};
use tracing::info;
use wiremock::MockServer;

use playwright_rs::{Browser, BrowserContext, LaunchOptions, Locator, Page, Playwright, expect};

/// Timeout for Playwright expect assertions.
/// Debug WASM binaries (~74 MB) take significant time to download and initialize,
/// especially on resource-constrained CI runners.
pub const EXPECT_TIMEOUT: Duration = Duration::from_secs(60);

use universal_inbox_api::{
    commands::generate::generate_testing_user,
    configuration::{AuthenticationSettings, LocalAuthenticationSettings, Settings},
    jobs::UniversalInboxJob,
    repository::{Repository, user::UserRepository},
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
    pub repository: Arc<Repository>,
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
            max_login_attempts: 5,
            login_attempt_window_seconds: 900,
            login_lockout_base_seconds: 60,
            login_lockout_max_seconds: 900,
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
    let repository = Arc::new(Repository::new(pool.clone()));
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
        repository,
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

    fill_and_submit_credentials(page, email, DEFAULT_PASSWORD, "login").await;

    // Wait for redirect away from login by checking that the notifications page is visible
    let notifications_page = page.locator("#notifications-page").await;
    expect(notifications_page)
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Notifications page not visible after login");
}

/// Fill the email + password fields and submit the form atomically.
///
/// The Dioxus form (`LoginPage` / `SignupPage`) uses controlled inputs
/// whose `value` attribute is bound to a signal. The submit handler reads
/// form values via the browser's `FormData` API (which sees the live DOM
/// value, not the signal), but two Dioxus-side races make the simple
/// "fill → click submit" pattern flaky:
///
/// 1. Between Playwright filling field A and field B, the parent can
///    re-render (e.g. when the email signal updates). Because the
///    password input is rendered with `value: "{props.value}"` and the
///    password signal is still empty, the re-render clobbers the input
///    back to "" — `fill_and_verify` recovers from this with a retry.
///
/// 2. Between the test confirming field B's value and the test's submit
///    click reaching the browser, another re-render can clobber the
///    value again — this time the click reads stale empty values from
///    `FormData`, the credentials parse fails, and the form silently
///    sets `force_validation = true` without making an API call. The
///    test then waits the full 60s for a redirect that never comes.
///
/// To close race (2), this helper finishes by re-setting both values
/// and calling `form.requestSubmit()` inside a single `page.evaluate`
/// — JavaScript runs to completion synchronously, so Dioxus cannot
/// schedule a re-render between the value writes and the submit event
/// (the submit handler runs synchronously inside that same JS turn).
pub async fn fill_and_submit_credentials(page: &Page, email: &str, password: &str, form: &str) {
    let email_input = page.locator("input[name='email']").await;
    expect(email_input.clone())
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .unwrap_or_else(|_| panic!("Email input not visible on {form} page"));

    fill_and_verify(&email_input, email, "email").await;

    let password_input = page.locator("input[name='password']").await;
    fill_and_verify(&password_input, password, "password").await;

    submit_form_atomic(page, email, password).await;
}

/// Re-write the email + password fields and submit the form in a single
/// JS execution. See `fill_and_submit_credentials` for the race this
/// protects against.
///
/// Uses the native `HTMLInputElement.value` setter rather than direct
/// property assignment so the change is visible to frameworks that
/// observe the prototype setter (the standard React/Dioxus controlled
/// -input integration pattern).
async fn submit_form_atomic(page: &Page, email: &str, password: &str) {
    // Escape values for JS string literals using JSON encoding — it
    // handles quotes, backslashes, and Unicode correctly.
    let email_js = serde_json::to_string(email).expect("encode email");
    let password_js = serde_json::to_string(password).expect("encode password");
    let js = format!(
        r#"(() => {{
            const email = document.querySelector('input[name="email"]');
            const password = document.querySelector('input[name="password"]');
            const form = email && email.closest('form');
            if (!email || !password || !form) {{
                throw new Error('login/signup form not found in DOM');
            }}
            const setter = Object.getOwnPropertyDescriptor(
                window.HTMLInputElement.prototype, 'value'
            ).set;
            setter.call(email, {email_js});
            setter.call(password, {password_js});
            email.dispatchEvent(new InputEvent('input', {{ bubbles: true }}));
            password.dispatchEvent(new InputEvent('input', {{ bubbles: true }}));
            form.requestSubmit();
        }})()"#
    );
    page.evaluate_expression(&js)
        .await
        .expect("Failed to submit form via JS");
}

/// Fill a Dioxus-controlled input and wait for the framework to observe
/// the new value. See `fill_and_submit_credentials` for the race this
/// guards against (specifically race #1).
pub async fn fill_and_verify(input: &Locator, value: &str, field: &str) {
    // Retry up to 3 times in case a re-render clobbers the typed value
    // before Dioxus's `oninput` handler picks it up. The `to_have_value`
    // assertion has its own internal polling, but it only resolves if the
    // value eventually appears; a re-render at the wrong moment can leave
    // the input permanently empty until we re-issue the fill.
    let mut last_err = String::new();
    for attempt in 0..3 {
        input
            .fill(value, None)
            .await
            .unwrap_or_else(|_| panic!("Failed to fill {field}"));
        let short_timeout = Duration::from_secs(5);
        let result = expect(input.clone())
            .with_timeout(short_timeout)
            .to_have_value(value)
            .await;
        if result.is_ok() {
            return;
        }
        last_err = format!("attempt {} of 3: {:?}", attempt + 1, result.err());
        let observed = input.input_value(None).await.unwrap_or_default();
        eprintln!(
            "fill_and_verify {field}: value mismatch (got {observed:?}, expected {value:?}), retrying"
        );
    }
    panic!("{field} value not synced to controlled input after 3 attempts: {last_err}");
}

/// Register a new user via the browser by filling the signup form.
pub async fn register(page: &Page, app_url: &str, email: &str) {
    page.goto(&format!("{app_url}/signup"), None)
        .await
        .expect("Failed to navigate to signup page");

    fill_and_submit_credentials(page, email, DEFAULT_PASSWORD, "signup").await;

    // Registration no longer auto-logs-in (email-enumeration hardening): the
    // signup page shows a generic confirmation message instead of redirecting.
    // Callers must explicitly `login` afterwards to reach the app.
    let confirmation = page.locator("#auth-confirmation").await;
    expect(confirmation)
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Confirmation message not visible after signup");
}

/// Mark a registered user's email as validated, server-side.
///
/// Registration stores an email-validation token and leaves the email
/// unverified; the frontend now gates the app behind validation (logging in
/// before verifying redirects to `/verify-email`). Tests that need to reach the
/// app simulate the user clicking the verification link by reading the stored
/// token and applying it directly through the user service.
pub async fn verify_user_email(app: &BrowserTestedApp, email: &str) {
    let email: EmailAddress = email.parse().expect("Test email should be valid");
    let mut transaction = app
        .repository
        .begin()
        .await
        .expect("Failed to begin transaction");

    let user = app
        .user_service
        .get_user_by_email(&mut transaction, &email)
        .await
        .expect("Failed to look up registered user")
        .expect("Registered user should exist");

    let token = app
        .repository
        .get_user_email_validation_token(&mut transaction, user.id)
        .await
        .expect("Failed to read email validation token")
        .expect("Registered user should have an email validation token");

    app.user_service
        .verify_email(&mut transaction, user.id, token)
        .await
        .expect("Failed to verify email");

    transaction
        .commit()
        .await
        .expect("Failed to commit email verification");
}

/// Wait until at least one notification row is visible in the DOM.
/// After login the notification list may still be loading from the API.
pub async fn wait_for_notification_rows(page: &Page) {
    let first_row = page.locator("#notifications-list .ui-nrow").await;
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
