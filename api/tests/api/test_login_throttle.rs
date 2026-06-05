//! Integration tests for the per-account login-attempt throttle (the second
//! brute-force protection layer, on top of the per-IP rate limit).
//!
//! The test fixture configures `max_login_attempts = 5`, so the 5th failed
//! password attempt locks the account (returning the generic 401 and sending
//! the lockout email once), and the 6th attempt is rejected with `429 Too Many
//! Requests` + `Retry-After` before credentials are even checked.
//!
//! OIDC / passkey logins are not exercised here because the throttle is scoped
//! to the local-password handler (`POST /users/me`); other auth flows use
//! distinct endpoints that never call it.

use email_address::EmailAddress;
use reqwest::{Client, StatusCode};
use rstest::*;
use uuid::Uuid;

use universal_inbox::user::User;
use universal_inbox_api::mailer::EmailTemplate;

use crate::helpers::{
    TestedApp, tested_app_with_local_auth,
    user::{create_user, login_user_response},
};

const PASSWORD: &str = "Very-harD-pasSword-5";
const MAX_ATTEMPTS: usize = 5;

fn client() -> Client {
    Client::builder().cookie_store(true).build().unwrap()
}

/// The throttle is Redis-backed and Redis is shared across test app instances
/// (unlike the in-memory per-IP governor), so a fixed email would inherit
/// failed-attempt state from earlier runs within the counter's TTL. Each test
/// uses a unique address to stay isolated and deterministic.
fn unique_email(prefix: &str) -> EmailAddress {
    format!("{prefix}-{}@example.com", Uuid::new_v4())
        .parse()
        .unwrap()
}

fn count_lockout_emails(emails: &[(User, EmailTemplate)]) -> usize {
    emails
        .iter()
        .filter(|(_, template)| matches!(template, EmailTemplate::AccountLockout { .. }))
        .count()
}

#[rstest]
#[tokio::test]
async fn test_account_locks_after_max_failed_attempts(
    #[future] tested_app_with_local_auth: TestedApp,
) {
    let app = tested_app_with_local_auth.await;
    let client = client();
    let email = unique_email("lockme");
    create_user(&app, email.clone(), PASSWORD).await;

    // The first `MAX_ATTEMPTS` wrong passwords return the generic 401. The
    // last of these crosses the threshold and locks the account.
    for attempt in 1..=MAX_ATTEMPTS {
        let response = login_user_response(&client, &app, email.clone(), "wrong-password").await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "attempt {attempt} should be a generic 401"
        );
    }

    // The next attempt is throttled before credentials are checked.
    let response = login_user_response(&client, &app, email.clone(), "wrong-password").await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(
        response
            .headers()
            .contains_key(reqwest::header::RETRY_AFTER),
        "429 response must carry a Retry-After header"
    );

    // Even the correct password is refused while locked.
    let response = login_user_response(&client, &app, email.clone(), PASSWORD).await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

    // Exactly one lockout email was sent, to the real account owner.
    let emails_sent = (*app.mailer_stub.read().await.emails_sent.read().await).clone();
    assert_eq!(count_lockout_emails(&emails_sent), 1);
}

#[rstest]
#[tokio::test]
async fn test_successful_login_resets_counter(#[future] tested_app_with_local_auth: TestedApp) {
    let app = tested_app_with_local_auth.await;
    let client = client();
    let email = unique_email("resetme");
    create_user(&app, email.clone(), PASSWORD).await;

    // Fail just short of the threshold.
    for _ in 1..MAX_ATTEMPTS {
        let response = login_user_response(&client, &app, email.clone(), "wrong-password").await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    // A correct login succeeds and clears the failed-attempt counter.
    let response = login_user_response(&client, &app, email.clone(), PASSWORD).await;
    assert_eq!(response.status(), StatusCode::OK);

    // With the counter reset, another full run short of the threshold stays at
    // 401 — never 429. (Without the reset, the accumulated count would lock.)
    for _ in 1..MAX_ATTEMPTS {
        let response = login_user_response(&client, &app, email.clone(), "wrong-password").await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "counter should have been reset by the successful login"
        );
    }

    // No account was ever locked, so no lockout email was sent.
    let emails_sent = (*app.mailer_stub.read().await.emails_sent.read().await).clone();
    assert_eq!(count_lockout_emails(&emails_sent), 0);
}

#[rstest]
#[tokio::test]
async fn test_unknown_account_is_throttled_without_enumeration(
    #[future] tested_app_with_local_auth: TestedApp,
) {
    let app = tested_app_with_local_auth.await;
    let client = client();
    // This email is never registered.
    let email = unique_email("ghost");

    for attempt in 1..=MAX_ATTEMPTS {
        let response = login_user_response(&client, &app, email.clone(), "wrong-password").await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "attempt {attempt} for an unknown email must look exactly like a real failed login"
        );
    }

    // The unknown email is throttled identically to a real locked account:
    // same 429, same Retry-After. An attacker cannot tell the two apart.
    let response = login_user_response(&client, &app, email.clone(), "wrong-password").await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(
        response
            .headers()
            .contains_key(reqwest::header::RETRY_AFTER)
    );

    // ...but no lockout email is sent, because no account exists. Nothing leaks.
    let emails_sent = (*app.mailer_stub.read().await.emails_sent.read().await).clone();
    assert_eq!(count_lockout_emails(&emails_sent), 0);
}
