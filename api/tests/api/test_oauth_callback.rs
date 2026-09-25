use rstest::*;
use secrecy::ExposeSecret;
use serde_json::Value;

use universal_inbox::{
    auth::auth_token::AuthenticationToken,
    integration_connection::{
        IntegrationConnectionId, IntegrationConnectionStatus, config::IntegrationConnectionConfig,
        integrations::ticktick::TickTickConfig,
    },
};

use crate::helpers::{
    TestedApp,
    auth::{AuthenticatedApp, authenticate_user, authenticated_app},
    integration_connection::{create_integration_connection, get_integration_connection},
    tested_app,
};

fn no_redirect_client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder().redirect(reqwest::redirect::Policy::none())
}

/// Start a TickTick OAuth flow as the authenticated user and return the
/// `state` carried by the provider authorize URL.
async fn start_oauth_flow(app: &AuthenticatedApp) -> (IntegrationConnectionId, String) {
    let integration_connection = create_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::TickTick(TickTickConfig::enabled()),
        IntegrationConnectionStatus::Created,
        None,
        None,
        None,
        None,
        None,
    )
    .await;

    let body: Value = app
        .client
        .get(format!(
            "{}oauth/authorize-url/{}",
            app.app.api_address, integration_connection.id
        ))
        .send()
        .await
        .expect("Failed to start OAuth authorization")
        .json()
        .await
        .expect("Failed to parse authorize-url response");
    let authorization_url = url::Url::parse(
        body["authorization_url"]
            .as_str()
            .expect("authorization_url"),
    )
    .expect("Invalid authorization URL");
    let state = authorization_url
        .query_pairs()
        .find(|(key, _)| key == "state")
        .expect("Missing state in authorization URL")
        .1
        .to_string();
    (integration_connection.id, state)
}

fn callback_location(response: &reqwest::Response) -> String {
    response
        .headers()
        .get(reqwest::header::LOCATION)
        .expect("expected Location header on redirect")
        .to_str()
        .expect("Location header must be ASCII")
        .to_string()
}

/// A callback carrying a state issued to another user must be refused before
/// the authorization code is exchanged: otherwise a victim following the
/// attacker's provider link would bind their provider account to the
/// attacker's connection.
#[rstest]
#[tokio::test]
async fn test_oauth_callback_rejects_state_issued_to_another_user(
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let (integration_connection_id, state) = start_oauth_flow(&app).await;

    // The victim is signed in to Universal Inbox with their own account.
    let (victim_client, _) =
        authenticate_user(&app.app, "5678", "Jane", "Roe", "jane@example.com").await;
    let victim_api_key: AuthenticationToken = victim_client
        .post(format!(
            "{}users/me/authentication-tokens",
            app.app.api_address
        ))
        .send()
        .await
        .expect("Failed to create API key")
        .json()
        .await
        .expect("Failed to deserialize API key response");

    let response = no_redirect_client_builder()
        .build()
        .unwrap()
        .get(format!(
            "{}/api/oauth/callback?state={}&code=victim-code",
            app.app.app_address.trim_end_matches('/'),
            urlencoding::encode(&state)
        ))
        .bearer_auth(&victim_api_key.jwt_token.expose_secret().0)
        .send()
        .await
        .expect("Failed to execute /api/oauth/callback request");

    assert_eq!(response.status(), reqwest::StatusCode::FOUND);
    let front_base_url = app.app.front_base_url.as_str().trim_end_matches('/');
    // `invalid-state`, not `provider-error`: the code was never exchanged.
    assert_eq!(
        callback_location(&response),
        format!("{front_base_url}/settings?oauth_error=invalid-state")
    );

    let integration_connection = get_integration_connection(&app, integration_connection_id)
        .await
        .expect("Integration connection must still exist");
    assert_eq!(
        integration_connection.status,
        IntegrationConnectionStatus::Created
    );
}

/// An anonymous callback (no Universal Inbox session) is refused without
/// consuming the state.
#[rstest]
#[tokio::test]
async fn test_oauth_callback_requires_a_session(#[future] authenticated_app: AuthenticatedApp) {
    let app = authenticated_app.await;
    let (_, state) = start_oauth_flow(&app).await;

    let response = no_redirect_client_builder()
        .build()
        .unwrap()
        .get(format!(
            "{}/api/oauth/callback?state={}&code=some-code",
            app.app.app_address.trim_end_matches('/'),
            urlencoding::encode(&state)
        ))
        .send()
        .await
        .expect("Failed to execute /api/oauth/callback request");

    assert_eq!(response.status(), reqwest::StatusCode::FOUND);
    let front_base_url = app.app.front_base_url.as_str().trim_end_matches('/');
    assert_eq!(
        callback_location(&response),
        format!("{front_base_url}/settings?oauth_error=invalid-state")
    );

    let state_key = format!("universal-inbox::oauth-state::{state}");
    let mut conn = app.app.cache.connection_manager.clone();
    let still_stored: Option<String> = redis::AsyncCommands::get(&mut conn, &state_key)
        .await
        .expect("Failed to read OAuth state");
    assert!(
        still_stored.is_some(),
        "an anonymous callback must not consume the state"
    );
}

/// `/api/oauth/callback` used to inline the
/// `format!("{err}")` chain into the `oauth_error` query parameter of the
/// redirect, leaking internal context (Redis lookup failures, integration
/// connection IDs, provider error blobs) into the user-visible URL.
///
/// The fix maps internal errors to a small enum of public reason codes
/// (`invalid-state`, `expired-state`, `provider-error`, `internal-error`) and
/// logs the full chain server-side via `tracing::error!`. Only the code
/// reaches the URL.
#[rstest]
#[tokio::test]
async fn test_oauth_callback_redacts_internal_error_chain(#[future] tested_app: TestedApp) {
    let app = tested_app.await;

    // Use a state value that does not exist in Redis. The service raises
    // `Unauthorized("Invalid or expired OAuth state")` for this case — our
    // classifier maps it to `invalid-state`, and crucially the original error
    // string must not appear in the redirect.
    let client = reqwest::Client::builder()
        // Capture the redirect ourselves instead of following it.
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("Failed to build HTTP client");

    let url = format!(
        "{}/api/oauth/callback?state=does-not-exist&code=irrelevant",
        app.app_address
    );

    let response = client
        .get(&url)
        .send()
        .await
        .expect("Failed to execute /api/oauth/callback request");

    assert_eq!(
        response.status(),
        reqwest::StatusCode::FOUND,
        "expected a 302 redirect"
    );

    let location = response
        .headers()
        .get(reqwest::header::LOCATION)
        .expect("expected Location header on redirect")
        .to_str()
        .expect("Location header must be ASCII");

    let front_base_url = app.front_base_url.as_str().trim_end_matches('/');
    let expected = format!("{front_base_url}/settings?oauth_error=invalid-state");
    assert_eq!(
        location, expected,
        "redirect Location must contain only the sanitized code, got: {location}"
    );

    // Defensive assertions: even if the redirect URL changes shape in the
    // future, none of the strings we used to leak should ever appear.
    assert!(
        !location.contains("Failed to retrieve"),
        "redirect leaks internal context: {location}"
    );
    assert!(
        !location.to_ascii_lowercase().contains("redis"),
        "redirect leaks Redis context: {location}"
    );
    assert!(
        !location.to_ascii_lowercase().contains("invalid or expired"),
        "redirect leaks raw error message: {location}"
    );
}

/// When the upstream OAuth provider returns its own error in the callback
/// query string (e.g. `error=access_denied`), we surface a generic
/// `provider-error` code instead of echoing the raw upstream value back to the
/// user. This prevents an attacker from crafting a callback URL that renders
/// arbitrary text in the user's URL bar / SPA toast.
#[rstest]
#[tokio::test]
async fn test_oauth_callback_redacts_provider_error_parameter(#[future] tested_app: TestedApp) {
    let app = tested_app.await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("Failed to build HTTP client");

    // The previous implementation passed `error` straight through, so an
    // attacker-controlled value like `<script>` or a long inflammatory string
    // would land in the user's URL bar verbatim.
    let raw_provider_error = "access_denied: user clicked deny on consent screen";
    let url = format!(
        "{}/api/oauth/callback?error={}",
        app.app_address,
        urlencoding::encode(raw_provider_error)
    );

    let response = client
        .get(&url)
        .send()
        .await
        .expect("Failed to execute /api/oauth/callback request");

    assert_eq!(response.status(), reqwest::StatusCode::FOUND);

    let location = response
        .headers()
        .get(reqwest::header::LOCATION)
        .expect("expected Location header")
        .to_str()
        .expect("Location header must be ASCII");

    let front_base_url = app.front_base_url.as_str().trim_end_matches('/');
    assert_eq!(
        location,
        format!("{front_base_url}/settings?oauth_error=provider-error"),
    );
    assert!(
        !location.contains("access_denied"),
        "redirect echoes attacker-controlled provider error: {location}"
    );
    assert!(
        !location.contains("consent screen"),
        "redirect echoes raw upstream text: {location}"
    );
}

/// Missing `code` / `state` parameters indicate a malformed callback — surface
/// `invalid-state` instead of echoing a custom `missing_code` / `missing_state`
/// string that mixed casing styles and could grow over time.
#[rstest]
#[tokio::test]
async fn test_oauth_callback_missing_state_returns_invalid_state(#[future] tested_app: TestedApp) {
    let app = tested_app.await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("Failed to build HTTP client");

    let url = format!("{}/api/oauth/callback?code=irrelevant", app.app_address);
    let response = client
        .get(&url)
        .send()
        .await
        .expect("Failed to execute /api/oauth/callback request");

    assert_eq!(response.status(), reqwest::StatusCode::FOUND);
    let location = response
        .headers()
        .get(reqwest::header::LOCATION)
        .expect("expected Location header")
        .to_str()
        .expect("Location header must be ASCII");

    let front_base_url = app.front_base_url.as_str().trim_end_matches('/');
    assert_eq!(
        location,
        format!("{front_base_url}/settings?oauth_error=invalid-state"),
    );
}
