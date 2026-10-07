//! Integration tests for the recent-authentication window: sensitive account
//! operations need a login or re-authentication more recent than the window
//! (ASVS 3.7.1), and API bearer tokens can never run them.

use std::time::Duration;

use reqwest::{Client, StatusCode};
use rstest::*;
use secrecy::{ExposeSecret, SecretBox};
use serde_json::Value;
use uuid::Uuid;
use webauthn_authenticator_rs::{WebauthnAuthenticator, softpasskey::SoftPasskey};
use webauthn_rs::prelude::RequestChallengeResponse;

use universal_inbox::{
    auth::{AuthorizeSessionResponse, auth_token::AuthenticationToken},
    pii::Pii,
    user::{Password, REAUTHENTICATION_REQUIRED_CODE, UserAuthKind, UserPatch},
};

use crate::helpers::{
    TEST_REAUTHENTICATION_WINDOW_SECONDS, TestedApp,
    auth::{authenticate_user, mock_oidc_openid_configuration},
    tested_app_with_domain_blacklist, tested_app_with_short_reauthentication_window,
    user::{
        add_local_auth_response, finish_add_passkey_registration_response, finish_body_with_nonce,
        front_origin_header, patch_user_response, register_user, remove_auth_method_response,
        split_creation_challenge, start_add_passkey_registration_response,
    },
};

const PASSWORD: &str = "Very-harD-pasSword-5";

/// The login throttle lives in a Redis shared by every test: one address per
/// test keeps their attempts apart.
fn unique_email() -> Pii<email_address::EmailAddress> {
    format!("reauth-{}@example.com", Uuid::new_v4())
        .parse()
        .unwrap()
}

async fn let_session_go_stale() {
    tokio::time::sleep(Duration::from_millis(
        u64::from(TEST_REAUTHENTICATION_WINDOW_SECONDS) * 1_000 + 200,
    ))
    .await;
}

async fn create_api_token_response(client: &Client, app: &TestedApp) -> reqwest::Response {
    client
        .post(format!("{}users/me/authentication-tokens", app.api_address))
        .header(reqwest::header::ORIGIN, front_origin_header(app))
        .send()
        .await
        .unwrap()
}

async fn reauthenticate_with_password_response(
    client: &Client,
    app: &TestedApp,
    password: &str,
) -> reqwest::Response {
    client
        .post(format!(
            "{}users/me/reauthentication/password",
            app.api_address
        ))
        .header(reqwest::header::ORIGIN, front_origin_header(app))
        .json(&SecretBox::new(Box::new(Password(password.to_string()))))
        .send()
        .await
        .unwrap()
}

async fn assert_reauthentication_required(response: reqwest::Response) {
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["code"], REAUTHENTICATION_REQUIRED_CODE);
}

#[rstest]
#[tokio::test]
async fn test_sensitive_operations_need_a_recent_authentication(
    #[future] tested_app_with_short_reauthentication_window: TestedApp,
) {
    let app = tested_app_with_short_reauthentication_window.await;
    let (client, _user) = register_user(&app, unique_email(), PASSWORD).await;
    let_session_go_stale().await;

    let email_change = UserPatch {
        email: Some(unique_email()),
        ..Default::default()
    };
    assert_reauthentication_required(patch_user_response(&client, &app, &email_change).await).await;
    assert_reauthentication_required(add_local_auth_response(&client, &app, PASSWORD).await).await;
    assert_reauthentication_required(
        start_add_passkey_registration_response(&client, &app, "stale_passkey").await,
    )
    .await;
    assert_reauthentication_required(
        remove_auth_method_response(&client, &app, UserAuthKind::Local).await,
    )
    .await;
    assert_reauthentication_required(create_api_token_response(&client, &app).await).await;
    let response = client
        .get(format!("{}auth/link-oidc/authorize", app.api_address))
        .send()
        .await
        .unwrap();
    assert_reauthentication_required(response).await;
}

#[rstest]
#[tokio::test]
async fn test_google_reauthentication_redirects_to_google(
    #[future] tested_app_with_domain_blacklist: TestedApp,
) {
    let app = tested_app_with_domain_blacklist.await;
    mock_oidc_openid_configuration(&app).await;
    let (client, _user) = authenticate_user(&app, "1234", "John", "Doe", "john@example.com").await;

    let response = client
        .get(format!(
            "{}auth/reauthenticate-oidc/authorize",
            app.api_address
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: AuthorizeSessionResponse = response.json().await.unwrap();
    assert!(
        body.authorization_url
            .query_pairs()
            .any(|(key, value)| key == "prompt" && value == "select_account"),
        "{}",
        body.authorization_url
    );
}

#[rstest]
#[tokio::test]
async fn test_a_name_change_does_not_need_a_recent_authentication(
    #[future] tested_app_with_short_reauthentication_window: TestedApp,
) {
    let app = tested_app_with_short_reauthentication_window.await;
    let email = unique_email();
    let (client, _user) = register_user(&app, email.clone(), PASSWORD).await;
    let_session_go_stale().await;

    // The profile form sends the unchanged email along with the name
    let patch = UserPatch {
        first_name: Some(Pii::new("John".to_string())),
        email: Some(email),
        ..Default::default()
    };
    let response = patch_user_response(&client, &app, &patch).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[rstest]
#[tokio::test]
async fn test_password_reauthentication_unlocks_sensitive_operations(
    #[future] tested_app_with_short_reauthentication_window: TestedApp,
) {
    let app = tested_app_with_short_reauthentication_window.await;
    let (client, _user) = register_user(&app, unique_email(), PASSWORD).await;
    let_session_go_stale().await;

    let response = reauthenticate_with_password_response(&client, &app, "Wrong-pasSword-5").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_reauthentication_required(create_api_token_response(&client, &app).await).await;

    let response = reauthenticate_with_password_response(&client, &app, PASSWORD).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = create_api_token_response(&client, &app).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[rstest]
#[tokio::test]
async fn test_a_wrong_reauthentication_password_does_not_log_the_user_out(
    #[future] tested_app_with_short_reauthentication_window: TestedApp,
) {
    let app = tested_app_with_short_reauthentication_window.await;
    let (client, _user) = register_user(&app, unique_email(), PASSWORD).await;
    let response = reauthenticate_with_password_response(&client, &app, "Wrong-pasSword-5").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let response = client
        .get(format!("{}users/me", app.api_address))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[rstest]
#[tokio::test]
async fn test_passkey_reauthentication_unlocks_sensitive_operations(
    #[future] tested_app_with_short_reauthentication_window: TestedApp,
) {
    let app = tested_app_with_short_reauthentication_window.await;
    let (client, _user) = register_user(&app, unique_email(), PASSWORD).await;
    let origin = app.front_base_url.clone();
    let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));

    // Add a passkey while the login is fresh
    let response = start_add_passkey_registration_response(&client, &app, "reauth_passkey").await;
    assert_eq!(response.status(), StatusCode::OK);
    let (creation_challenge, nonce) = split_creation_challenge(&response.text().await.unwrap());
    let register_credential = authenticator
        .do_registration(origin.clone(), creation_challenge)
        .unwrap();
    let response =
        finish_add_passkey_registration_response(&client, &app, &register_credential, &nonce).await;
    assert_eq!(response.status(), StatusCode::OK);

    let_session_go_stale().await;
    assert_reauthentication_required(create_api_token_response(&client, &app).await).await;

    let response = client
        .post(format!(
            "{}users/me/reauthentication/passkey/start",
            app.api_address
        ))
        .header(reqwest::header::ORIGIN, front_origin_header(&app))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.unwrap();
    let nonce = serde_json::from_str::<Value>(&body).unwrap()["nonce"]
        .as_str()
        .unwrap()
        .to_string();
    let request_challenge: RequestChallengeResponse = serde_json::from_str(&body).unwrap();
    let credential = authenticator
        .do_authentication(origin, request_challenge)
        .unwrap();

    let response = client
        .post(format!(
            "{}users/me/reauthentication/passkey/finish",
            app.api_address
        ))
        .header(reqwest::header::ORIGIN, front_origin_header(&app))
        .json(&finish_body_with_nonce(&credential, &nonce))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = create_api_token_response(&client, &app).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[rstest]
#[tokio::test]
async fn test_an_api_token_cannot_run_sensitive_operations(
    #[future] tested_app_with_short_reauthentication_window: TestedApp,
) {
    let app = tested_app_with_short_reauthentication_window.await;
    let (client, _user) = register_user(&app, unique_email(), PASSWORD).await;
    let api_token: AuthenticationToken = create_api_token_response(&client, &app)
        .await
        .json()
        .await
        .unwrap();
    let jwt = api_token.jwt_token.expose_secret().0.clone();
    let bearer_client = Client::new();

    // Even right after the session that created it logged in
    let response = bearer_client
        .post(format!("{}users/me/authentication-tokens", app.api_address))
        .header(reqwest::header::ORIGIN, front_origin_header(&app))
        .bearer_auth(&jwt)
        .send()
        .await
        .unwrap();
    assert_reauthentication_required(response).await;

    let response = bearer_client
        .patch(format!("{}users/me", app.api_address))
        .bearer_auth(&jwt)
        .json(&UserPatch {
            email: Some(unique_email()),
            ..Default::default()
        })
        .send()
        .await
        .unwrap();
    assert_reauthentication_required(response).await;

    let response = bearer_client
        .delete(format!(
            "{}users/me/auth-methods/{}",
            app.api_address,
            UserAuthKind::Local
        ))
        .header(reqwest::header::ORIGIN, front_origin_header(&app))
        .bearer_auth(&jwt)
        .send()
        .await
        .unwrap();
    assert_reauthentication_required(response).await;
}
