use rstest::*;

use crate::helpers::{TestedApp, tested_app};

/// `/api/oauth/callback` is unauthenticated by design. It used to inline the
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
