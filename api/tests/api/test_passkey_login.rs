//! Integration tests for passkey login against a credential that was removed
//! or replaced after the login ceremony started: the account's current
//! credential is the only one allowed to log in.

use reqwest::{Client, StatusCode};
use rstest::*;
use serde_json::Value;
use uuid::Uuid;
use webauthn_authenticator_rs::{WebauthnAuthenticator, softpasskey::SoftPasskey};
use webauthn_rs::prelude::RequestChallengeResponse;

use universal_inbox::{pii::Pii, user::UserAuthKind};

use crate::helpers::{
    TestedApp, tested_app_with_local_auth,
    user::{
        finish_add_passkey_registration_response, finish_passkey_authentication_response,
        get_current_user_response, register_user, remove_auth_method_response,
        split_creation_challenge, start_add_passkey_registration_response,
        start_passkey_authentication_response,
    },
};

const PASSWORD: &str = "Very-harD-pasSword-5";

fn unique_email() -> Pii<email_address::EmailAddress> {
    format!("passkey-login-{}@example.com", Uuid::new_v4())
        .parse()
        .unwrap()
}

fn cookie_client() -> Client {
    Client::builder().cookie_store(true).build().unwrap()
}

fn split_request_challenge(body: &str) -> (RequestChallengeResponse, String) {
    let value: Value = serde_json::from_str(body).unwrap();
    let nonce = value["nonce"].as_str().unwrap().to_string();
    (serde_json::from_str(body).unwrap(), nonce)
}

/// Register a passkey for the logged-in `client` and return its username.
async fn add_passkey(
    client: &Client,
    app: &TestedApp,
    authenticator: &mut WebauthnAuthenticator<SoftPasskey>,
) -> String {
    let username = format!("passkey_{}", Uuid::new_v4().simple());
    let response = start_add_passkey_registration_response(client, app, &username).await;
    assert_eq!(response.status(), StatusCode::OK);
    let (creation_challenge, nonce) = split_creation_challenge(&response.text().await.unwrap());
    let credential = authenticator
        .do_registration(app.front_base_url.clone(), creation_challenge)
        .unwrap();
    let response = finish_add_passkey_registration_response(client, app, &credential, &nonce).await;
    assert_eq!(response.status(), StatusCode::OK);
    username
}

/// Start a login ceremony with a fresh (logged out) client.
async fn start_login(
    app: &TestedApp,
    username: &str,
) -> (Client, RequestChallengeResponse, String) {
    let login_client = cookie_client();
    let response = start_passkey_authentication_response(&login_client, app, username).await;
    assert_eq!(response.status(), StatusCode::OK);
    let (challenge, nonce) = split_request_challenge(&response.text().await.unwrap());
    (login_client, challenge, nonce)
}

async fn assert_not_logged_in(client: &Client, app: &TestedApp) {
    let response = get_current_user_response(client, app).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[rstest]
#[tokio::test]
async fn test_a_still_registered_passkey_logs_in(#[future] tested_app_with_local_auth: TestedApp) {
    let app = tested_app_with_local_auth.await;
    let (client, user) = register_user(&app, unique_email(), PASSWORD).await;
    let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
    let username = add_passkey(&client, &app, &mut authenticator).await;

    let (login_client, challenge, nonce) = start_login(&app, &username).await;
    let credential = authenticator
        .do_authentication(app.front_base_url.clone(), challenge)
        .unwrap();
    let response =
        finish_passkey_authentication_response(&login_client, &app, &credential, &nonce).await;

    assert_eq!(response.status(), StatusCode::OK);
    let response = get_current_user_response(&login_client, &app).await;
    assert_eq!(response.status(), StatusCode::OK);
    let current_user: universal_inbox::user::User = response.json().await.unwrap();
    assert_eq!(current_user.id, user.id);
}

#[rstest]
#[tokio::test]
async fn test_a_replaced_passkey_cannot_finish_a_login_started_before_the_replacement(
    #[future] tested_app_with_local_auth: TestedApp,
) {
    let app = tested_app_with_local_auth.await;
    let (client, _user) = register_user(&app, unique_email(), PASSWORD).await;
    let mut old_authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
    let username = add_passkey(&client, &app, &mut old_authenticator).await;

    // The login ceremony starts while the old passkey is registered...
    let (login_client, challenge, nonce) = start_login(&app, &username).await;

    // ...then the passkey is replaced (removed, then another one added).
    let response = remove_auth_method_response(&client, &app, UserAuthKind::Passkey).await;
    assert_eq!(response.status(), StatusCode::OK);
    let mut new_authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
    add_passkey(&client, &app, &mut new_authenticator).await;

    let credential = old_authenticator
        .do_authentication(app.front_base_url.clone(), challenge)
        .unwrap();
    let response =
        finish_passkey_authentication_response(&login_client, &app, &credential, &nonce).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_not_logged_in(&login_client, &app).await;
}

#[rstest]
#[tokio::test]
async fn test_a_removed_passkey_cannot_finish_a_pending_login(
    #[future] tested_app_with_local_auth: TestedApp,
) {
    let app = tested_app_with_local_auth.await;
    let (client, _user) = register_user(&app, unique_email(), PASSWORD).await;
    let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
    let username = add_passkey(&client, &app, &mut authenticator).await;

    let (login_client, challenge, nonce) = start_login(&app, &username).await;
    let response = remove_auth_method_response(&client, &app, UserAuthKind::Passkey).await;
    assert_eq!(response.status(), StatusCode::OK);

    let credential = authenticator
        .do_authentication(app.front_base_url.clone(), challenge)
        .unwrap();
    let response =
        finish_passkey_authentication_response(&login_client, &app, &credential, &nonce).await;

    assert!(
        response.status().is_client_error(),
        "unexpected status {}",
        response.status()
    );
    assert_not_logged_in(&login_client, &app).await;
}
