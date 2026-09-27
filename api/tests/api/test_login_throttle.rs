//! Integration tests for the per-account login-attempt throttle (the second
//! brute-force protection layer, on top of the per-IP rate limit).
//!
//! The test fixture configures `max_login_attempts = 5`, so the 5th failed
//! password attempt locks the account (returning the generic 401 and sending
//! the lockout email once), and the 6th attempt is rejected with `429 Too Many
//! Requests` + `Retry-After` before credentials are even checked.
//!
//! The per-account request budgets (login requests, and registration /
//! password-reset / verification-resend emails per address) use
//! `tested_app_with_account_rate_limits`, whose low limits are exhausted from
//! rotating client IPs to show that only the email is keyed on.
//!
//! OIDC / passkey logins are not exercised here because the throttle is scoped
//! to the local-password handler (`POST /users/me`); other auth flows use
//! distinct endpoints that never call it.

use email_address::EmailAddress;
use reqwest::{Client, StatusCode};
use rstest::*;
use secrecy::SecretBox;
use uuid::Uuid;

use universal_inbox::user::{Credentials, Password, RegisterUserParameters, User};
use universal_inbox_api::mailer::EmailTemplate;

use crate::helpers::{
    TEST_MAX_ACCOUNT_EMAILS_PER_ADDRESS, TEST_MAX_LOGIN_REQUESTS_PER_ACCOUNT, TestedApp,
    tested_app_with_account_rate_limits, tested_app_with_local_auth,
    user::{create_user, create_user_and_login, login_user_response},
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

/// A distinct client IP per request (the test config trusts one proxy hop), so
/// only the per-account budget, not the per-IP limiter, can throttle.
fn ip(n: u32) -> String {
    format!("198.51.100.{}", n % 250 + 1)
}

async fn send_password_reset_from(
    client: &Client,
    app: &TestedApp,
    email: &EmailAddress,
    ip: String,
) -> reqwest::Response {
    client
        .post(format!("{}users/password-reset", app.api_address))
        .header("X-Forwarded-For", ip)
        .json(email)
        .send()
        .await
        .unwrap()
}

async fn register_from(
    client: &Client,
    app: &TestedApp,
    email: &EmailAddress,
    ip: String,
) -> reqwest::Response {
    client
        .post(format!("{}users", app.api_address))
        .header("X-Forwarded-For", ip)
        .json(&RegisterUserParameters {
            credentials: Credentials {
                email: email.clone(),
                password: SecretBox::new(Box::new(Password(PASSWORD.to_string()))),
            },
        })
        .send()
        .await
        .unwrap()
}

async fn login_from(app: &TestedApp, email: &EmailAddress, ip: String) -> reqwest::Response {
    client()
        .post(format!("{}users/me", app.api_address))
        .header("X-Forwarded-For", ip)
        .json(&Credentials {
            email: email.clone(),
            password: SecretBox::new(Box::new(Password(PASSWORD.to_string()))),
        })
        .send()
        .await
        .unwrap()
}

/// Status, `Retry-After` presence and body of a response, to compare the
/// responses served for existing and unknown accounts.
async fn observable(response: reqwest::Response) -> (StatusCode, bool, String) {
    (
        response.status(),
        response
            .headers()
            .contains_key(reqwest::header::RETRY_AFTER),
        response.text().await.unwrap(),
    )
}

async fn emails_sent_to(app: &TestedApp, email: &EmailAddress) -> usize {
    app.mailer_stub
        .read()
        .await
        .emails_sent
        .read()
        .await
        .iter()
        .filter(|(user, _)| user.email.as_ref() == Some(email))
        .count()
}

#[rstest]
#[tokio::test]
async fn test_password_reset_is_rate_limited_per_email_across_ips(
    #[future] tested_app_with_account_rate_limits: TestedApp,
) {
    let app = tested_app_with_account_rate_limits.await;
    let client = client();
    let known = unique_email("reset-known");
    create_user(&app, known.clone(), PASSWORD).await;
    let unknown = unique_email("reset-unknown");

    let mut responses = Vec::new();
    for email in [&known, &unknown] {
        let mut observed = Vec::new();
        for n in 0..=TEST_MAX_ACCOUNT_EMAILS_PER_ADDRESS {
            let response = send_password_reset_from(&client, &app, email, ip(n)).await;
            observed.push(observable(response).await);
        }
        responses.push(observed);
    }

    let known_responses = &responses[0];
    for (status, _, _) in &known_responses[..TEST_MAX_ACCOUNT_EMAILS_PER_ADDRESS as usize] {
        assert_eq!(*status, StatusCode::OK);
    }
    let (status, has_retry_after, _) = known_responses.last().unwrap();
    assert_eq!(*status, StatusCode::TOO_MANY_REQUESTS);
    assert!(
        has_retry_after,
        "429 response must carry a Retry-After header"
    );
    assert_eq!(
        responses[0], responses[1],
        "unknown emails must get exactly the same responses as existing accounts"
    );
    assert_eq!(
        emails_sent_to(&app, &known).await,
        TEST_MAX_ACCOUNT_EMAILS_PER_ADDRESS as usize
    );
}

#[rstest]
#[tokio::test]
async fn test_registration_is_rate_limited_per_email_across_ips(
    #[future] tested_app_with_account_rate_limits: TestedApp,
) {
    let app = tested_app_with_account_rate_limits.await;
    let client = client();
    // The first registration creates the account; later ones hit the
    // "already registered" path, which emails the owner as well.
    let fresh = unique_email("register-fresh");
    let existing = unique_email("register-existing");
    create_user(&app, existing.clone(), PASSWORD).await;

    let mut responses = Vec::new();
    for email in [&fresh, &existing] {
        let mut observed = Vec::new();
        for n in 0..=TEST_MAX_ACCOUNT_EMAILS_PER_ADDRESS {
            let response = register_from(&client, &app, email, ip(n)).await;
            observed.push(observable(response).await);
        }
        responses.push(observed);
    }

    let fresh_responses = &responses[0];
    for (status, _, _) in &fresh_responses[..TEST_MAX_ACCOUNT_EMAILS_PER_ADDRESS as usize] {
        assert_eq!(*status, StatusCode::OK);
    }
    let (status, has_retry_after, _) = fresh_responses.last().unwrap();
    assert_eq!(*status, StatusCode::TOO_MANY_REQUESTS);
    assert!(
        has_retry_after,
        "429 response must carry a Retry-After header"
    );
    assert_eq!(
        responses[0], responses[1],
        "new and already-registered emails must get exactly the same responses"
    );
    assert_eq!(
        emails_sent_to(&app, &existing).await,
        TEST_MAX_ACCOUNT_EMAILS_PER_ADDRESS as usize
    );
}

#[rstest]
#[tokio::test]
async fn test_registration_and_password_reset_share_the_email_budget(
    #[future] tested_app_with_account_rate_limits: TestedApp,
) {
    let app = tested_app_with_account_rate_limits.await;
    let client = client();
    let email = unique_email("shared-budget");
    create_user(&app, email.clone(), PASSWORD).await;

    for n in 0..TEST_MAX_ACCOUNT_EMAILS_PER_ADDRESS {
        let response = if n % 2 == 0 {
            register_from(&client, &app, &email, ip(n)).await
        } else {
            send_password_reset_from(&client, &app, &email, ip(n)).await
        };
        assert_eq!(response.status(), StatusCode::OK);
    }

    let response = send_password_reset_from(
        &client,
        &app,
        &email,
        ip(TEST_MAX_ACCOUNT_EMAILS_PER_ADDRESS),
    )
    .await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    let response = register_from(
        &client,
        &app,
        &email,
        ip(TEST_MAX_ACCOUNT_EMAILS_PER_ADDRESS + 1),
    )
    .await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[rstest]
#[tokio::test]
async fn test_login_requests_are_rate_limited_per_account_across_ips(
    #[future] tested_app_with_account_rate_limits: TestedApp,
) {
    let app = tested_app_with_account_rate_limits.await;
    let email = unique_email("login-budget");
    create_user(&app, email.clone(), PASSWORD).await;

    // Successful logins count too: the budget caps every attempt, not only
    // the failures the lockout tracks.
    for n in 0..TEST_MAX_LOGIN_REQUESTS_PER_ACCOUNT {
        let response = login_from(&app, &email, ip(n)).await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "login {n} should succeed"
        );
    }

    let response = login_from(&app, &email, ip(TEST_MAX_LOGIN_REQUESTS_PER_ACCOUNT)).await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(
        response
            .headers()
            .contains_key(reqwest::header::RETRY_AFTER)
    );
}

async fn resend_verification_email_from(
    client: &Client,
    app: &TestedApp,
    ip: String,
) -> reqwest::Response {
    client
        .post(format!("{}users/me/email-verification", app.api_address))
        .header("X-Forwarded-For", ip)
        .send()
        .await
        .unwrap()
}

#[rstest]
#[tokio::test]
async fn test_verification_email_resend_is_rate_limited_per_email_across_ips(
    #[future] tested_app_with_account_rate_limits: TestedApp,
) {
    let app = tested_app_with_account_rate_limits.await;
    let email = unique_email("resend-verification");
    let (client, _user) = create_user_and_login(&app, email.clone(), PASSWORD).await;

    for n in 0..TEST_MAX_ACCOUNT_EMAILS_PER_ADDRESS {
        let response = resend_verification_email_from(&client, &app, ip(n)).await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "resend {n} should succeed"
        );
    }

    let response =
        resend_verification_email_from(&client, &app, ip(TEST_MAX_ACCOUNT_EMAILS_PER_ADDRESS))
            .await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(
        response
            .headers()
            .contains_key(reqwest::header::RETRY_AFTER),
        "429 response must carry a Retry-After header"
    );
    assert_eq!(
        emails_sent_to(&app, &email).await,
        TEST_MAX_ACCOUNT_EMAILS_PER_ADDRESS as usize
    );
}

#[rstest]
#[tokio::test]
async fn test_verification_email_resend_and_password_reset_share_the_email_budget(
    #[future] tested_app_with_account_rate_limits: TestedApp,
) {
    let app = tested_app_with_account_rate_limits.await;
    let email = unique_email("resend-shared-budget");
    let (client, _user) = create_user_and_login(&app, email.clone(), PASSWORD).await;

    for n in 0..TEST_MAX_ACCOUNT_EMAILS_PER_ADDRESS {
        let response = if n % 2 == 0 {
            resend_verification_email_from(&client, &app, ip(n)).await
        } else {
            send_password_reset_from(&client, &app, &email, ip(n)).await
        };
        assert_eq!(response.status(), StatusCode::OK);
    }

    let response = send_password_reset_from(
        &client,
        &app,
        &email,
        ip(TEST_MAX_ACCOUNT_EMAILS_PER_ADDRESS),
    )
    .await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    let response =
        resend_verification_email_from(&client, &app, ip(TEST_MAX_ACCOUNT_EMAILS_PER_ADDRESS + 1))
            .await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}
