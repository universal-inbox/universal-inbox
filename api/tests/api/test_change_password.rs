//! Integration tests for `PATCH /users/me/auth-methods/local`: an
//! authenticated local-auth user changes their password, which signs out their
//! other sessions and emails them.

use chrono::Utc;
use email_address::EmailAddress;
use reqwest::{Client, StatusCode};
use rstest::*;
use secrecy::SecretBox;
use universal_inbox::pii::Pii;
use uuid::Uuid;

use universal_inbox::user::{Password, PasswordChange};
use universal_inbox_api::mailer::EmailTemplate;

use crate::helpers::{
    TestedApp,
    auth::authenticate_user,
    tested_app_with_domain_blacklist, tested_app_with_local_auth,
    user::{
        create_user, create_user_and_login, front_origin_header, get_current_user_response,
        get_password_reset_token, login_user_response, reset_password_response,
    },
};

const PASSWORD: &str = "Very-harD-pasSword-5";
const NEW_PASSWORD: &str = "New-very-harD-pasSword-5";
/// Matches the `max_login_attempts` of the test fixture.
const MAX_ATTEMPTS: usize = 5;

/// The login throttle shared by the current password check is Redis-backed
/// and Redis is shared across test apps: use a unique address per test.
fn unique_email(prefix: &str) -> Pii<EmailAddress> {
    format!("{prefix}-{}@example.com", Uuid::new_v4())
        .parse()
        .unwrap()
}

fn client() -> Client {
    Client::builder().cookie_store(true).build().unwrap()
}

fn password_change(current_password: &str, new_password: &str) -> PasswordChange {
    PasswordChange {
        current_password: SecretBox::new(Box::new(Password(current_password.to_string()))),
        new_password: SecretBox::new(Box::new(Password(new_password.to_string()))),
    }
}

async fn change_password_response(
    client: &Client,
    app: &TestedApp,
    current_password: &str,
    new_password: &str,
) -> reqwest::Response {
    client
        .patch(format!("{}users/me/auth-methods/local", app.api_address))
        .header(reqwest::header::ORIGIN, front_origin_header(app))
        .json(&password_change(current_password, new_password))
        .send()
        .await
        .unwrap()
}

/// Session revocation has the one-second precision of the JWT `iat` claim: a
/// session issued in the same second as a password change survives it. Wait
/// for the next second so that sessions opened before the change are revoked.
async fn wait_for_next_second() {
    let millis_into_second = Utc::now().timestamp_subsec_millis() as u64;
    tokio::time::sleep(std::time::Duration::from_millis(1_001 - millis_into_second)).await;
}

async fn password_changed_emails_sent_to(app: &TestedApp, email: &Pii<EmailAddress>) -> usize {
    app.mailer_stub
        .read()
        .await
        .emails_sent
        .read()
        .await
        .iter()
        .filter(|(user, template)| {
            user.email.as_ref() == Some(email)
                && matches!(template, EmailTemplate::PasswordChanged { .. })
        })
        .count()
}

#[rstest]
#[tokio::test]
async fn test_change_password(#[future] tested_app_with_local_auth: TestedApp) {
    let app = tested_app_with_local_auth.await;
    let email = unique_email("change");
    let (client, _user) = create_user_and_login(&app, email.clone(), PASSWORD).await;

    let response = change_password_response(&client, &app, PASSWORD, NEW_PASSWORD).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // The caller stays logged in.
    let response = get_current_user_response(&client, &app).await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = login_user_response(&self::client(), &app, email.clone(), PASSWORD).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = login_user_response(&self::client(), &app, email.clone(), NEW_PASSWORD).await;
    assert_eq!(response.status(), StatusCode::OK);

    assert_eq!(password_changed_emails_sent_to(&app, &email).await, 1);
}

#[rstest]
#[tokio::test]
async fn test_change_password_revokes_other_sessions(
    #[future] tested_app_with_local_auth: TestedApp,
) {
    let app = tested_app_with_local_auth.await;
    let email = unique_email("revoke");
    let (client, _user) = create_user_and_login(&app, email.clone(), PASSWORD).await;
    let other_client = self::client();
    let response = login_user_response(&other_client, &app, email.clone(), PASSWORD).await;
    assert_eq!(response.status(), StatusCode::OK);

    wait_for_next_second().await;
    let response = change_password_response(&client, &app, PASSWORD, NEW_PASSWORD).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = get_current_user_response(&other_client, &app).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = get_current_user_response(&client, &app).await;
    assert_eq!(response.status(), StatusCode::OK);

    // A session opened after the change is valid.
    let new_client = self::client();
    let response = login_user_response(&new_client, &app, email, NEW_PASSWORD).await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = get_current_user_response(&new_client, &app).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[rstest]
#[tokio::test]
async fn test_change_password_with_wrong_current_password(
    #[future] tested_app_with_local_auth: TestedApp,
) {
    let app = tested_app_with_local_auth.await;
    let email = unique_email("wrong-current");
    let (client, _user) = create_user_and_login(&app, email.clone(), PASSWORD).await;

    let response = change_password_response(&client, &app, "wrong-password", NEW_PASSWORD).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        response
            .text()
            .await
            .unwrap()
            .contains("The current password is incorrect")
    );

    // A wrong current password does not log the user out, and the password
    // is unchanged.
    let response = get_current_user_response(&client, &app).await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = login_user_response(&self::client(), &app, email.clone(), PASSWORD).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(password_changed_emails_sent_to(&app, &email).await, 0);
}

#[rstest]
#[tokio::test]
async fn test_change_password_is_throttled_like_login(
    #[future] tested_app_with_local_auth: TestedApp,
) {
    let app = tested_app_with_local_auth.await;
    let email = unique_email("throttle");
    let (client, _user) = create_user_and_login(&app, email.clone(), PASSWORD).await;

    for attempt in 1..=MAX_ATTEMPTS {
        let response =
            change_password_response(&client, &app, "wrong-password", NEW_PASSWORD).await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "attempt {attempt} should be rejected as invalid input"
        );
    }

    // The account is now locked: even the right current password is refused.
    let response = change_password_response(&client, &app, PASSWORD, NEW_PASSWORD).await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(
        response
            .headers()
            .contains_key(reqwest::header::RETRY_AFTER)
    );
}

#[rstest]
#[case::too_short("short")]
#[case::same_as_current(PASSWORD)]
#[tokio::test]
async fn test_change_password_rejects_invalid_new_password(
    #[future] tested_app_with_local_auth: TestedApp,
    #[case] new_password: &str,
) {
    let app = tested_app_with_local_auth.await;
    let email = unique_email("invalid-new");
    let (client, _user) = create_user_and_login(&app, email.clone(), PASSWORD).await;

    let response = change_password_response(&client, &app, PASSWORD, new_password).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = login_user_response(&self::client(), &app, email, PASSWORD).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[rstest]
#[tokio::test]
async fn test_change_password_without_local_auth(
    #[future] tested_app_with_domain_blacklist: TestedApp,
) {
    let app = tested_app_with_domain_blacklist.await;
    let (client, _user) = authenticate_user(&app, "1234", "John", "Doe", "test@example.com").await;

    let response = change_password_response(&client, &app, PASSWORD, NEW_PASSWORD).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = get_current_user_response(&client, &app).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[rstest]
#[tokio::test]
async fn test_change_password_requires_front_origin(
    #[future] tested_app_with_local_auth: TestedApp,
) {
    let app = tested_app_with_local_auth.await;
    let email = unique_email("origin");
    let (client, _user) = create_user_and_login(&app, email.clone(), PASSWORD).await;

    let response = client
        .patch(format!("{}users/me/auth-methods/local", app.api_address))
        .json(&password_change(PASSWORD, NEW_PASSWORD))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = login_user_response(&self::client(), &app, email, PASSWORD).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[rstest]
#[tokio::test]
async fn test_change_password_unauthenticated(#[future] tested_app_with_local_auth: TestedApp) {
    let app = tested_app_with_local_auth.await;

    let response = change_password_response(&client(), &app, PASSWORD, NEW_PASSWORD).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[rstest]
#[tokio::test]
async fn test_reset_password_revokes_sessions(#[future] tested_app_with_local_auth: TestedApp) {
    let app = tested_app_with_local_auth.await;
    let email = unique_email("reset");
    let user = create_user(&app, email.clone(), PASSWORD).await;
    let logged_in_client = self::client();
    let response = login_user_response(&logged_in_client, &app, email.clone(), PASSWORD).await;
    assert_eq!(response.status(), StatusCode::OK);

    let anonymous_client = self::client();
    let response = anonymous_client
        .post(format!("{}users/password-reset", app.api_address))
        .json(&email)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let password_reset_token = get_password_reset_token(&app, user.id).await.unwrap();

    wait_for_next_second().await;
    let response = reset_password_response(
        &anonymous_client,
        &app,
        user.id,
        password_reset_token,
        NEW_PASSWORD,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = get_current_user_response(&logged_in_client, &app).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
