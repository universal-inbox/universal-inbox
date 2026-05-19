use email_address::EmailAddress;
use reqwest::Client;
use secrecy::SecretBox;
use serde_json::Value;
use webauthn_rs::prelude::{CreationChallengeResponse, RegisterPublicKeyCredential};

use universal_inbox::{
    auth::SessionAuthValidationParameters,
    user::{
        Credentials, EmailValidationToken, Password, PasswordResetToken, RegisterUserParameters,
        User, UserAuthKind, UserAuthMethod, UserId, UserPatch, Username,
    },
};

use universal_inbox_api::{
    repository::user::UserRepository,
    universal_inbox::user::model::{LocalUserAuth, UserAuth},
};

use crate::helpers::TestedApp;

/// Helper: return the test app's front-end Origin header value.
/// All `/passkeys/*` and `/auth-methods/*` endpoints reject requests
/// whose `Origin` (or `Referer`) does not match the configured
/// `application.front_base_url` (universal-inbox-bkj.32).
pub fn front_origin_header(app: &TestedApp) -> String {
    app.front_base_url.origin().ascii_serialization()
}

/// Parse a passkey start response into the underlying
/// `CreationChallengeResponse` plus the server-generated nonce that
/// must be echoed back on finish. The body shape post-bkj.32 is
/// `{"publicKey": {...}, "nonce": "..."}`.
pub fn split_creation_challenge(body: &str) -> (CreationChallengeResponse, String) {
    let value: Value = serde_json::from_str(body).expect("Start response must be JSON object");
    let nonce = value
        .get("nonce")
        .and_then(|v| v.as_str())
        .expect("Start response must include a `nonce` field")
        .to_string();
    let challenge: CreationChallengeResponse =
        serde_json::from_str(body).expect("Start response must deserialize as challenge");
    (challenge, nonce)
}

/// Build the JSON body for a passkey finish call: serialize the
/// `RegisterPublicKeyCredential` (or `PublicKeyCredential`) and splice
/// in the `nonce` field at the top level.
pub fn finish_body_with_nonce<T: serde::Serialize>(credential: &T, nonce: &str) -> Value {
    let mut value = serde_json::to_value(credential).expect("Credential serializes to JSON object");
    if let Some(obj) = value.as_object_mut() {
        obj.insert("nonce".to_string(), Value::String(nonce.to_string()));
    }
    value
}

pub async fn register_user_response(
    client: &Client,
    app: &TestedApp,
    email: EmailAddress,
    password: &str,
) -> reqwest::Response {
    client
        .post(format!("{}users", app.api_address))
        .json(&RegisterUserParameters {
            credentials: Credentials {
                email,
                password: SecretBox::new(Box::new(Password(password.to_string()))),
            },
        })
        .send()
        .await
        .unwrap()
}

pub async fn register_user(app: &TestedApp, email: EmailAddress, password: &str) -> (Client, User) {
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .build()
        .unwrap();

    let response = register_user_response(&client, app, email, password).await;

    assert_eq!(response.status(), 200);

    let user: User = get_current_user_response(&client, app)
        .await
        .json()
        .await
        .unwrap();

    (client, user)
}

pub async fn get_current_user_response(client: &Client, app: &TestedApp) -> reqwest::Response {
    client
        .get(format!("{}users/me", app.api_address))
        .send()
        .await
        .unwrap()
}

pub async fn get_current_user(client: &Client, app: &TestedApp) -> User {
    get_current_user_response(client, app)
        .await
        .json()
        .await
        .unwrap()
}

pub async fn login_user_response(
    client: &Client,
    app: &TestedApp,
    email: EmailAddress,
    password: &str,
) -> reqwest::Response {
    client
        .post(format!("{}users/me", app.api_address))
        .json(&Credentials {
            email,
            password: SecretBox::new(Box::new(Password(password.to_string()))),
        })
        .send()
        .await
        .unwrap()
}

pub async fn logout_user_response(client: &Client, api_address: &str) -> reqwest::Response {
    client
        .delete(format!("{api_address}auth/session"))
        .send()
        .await
        .unwrap()
}

pub async fn get_user_email_validation_token(
    app: &TestedApp,
    user_id: UserId,
) -> Option<EmailValidationToken> {
    let mut transaction = app.repository.begin().await.unwrap();
    let token = app
        .repository
        .get_user_email_validation_token(&mut transaction, user_id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    token
}

pub async fn get_password_reset_token(
    app: &TestedApp,
    user_id: UserId,
) -> Option<PasswordResetToken> {
    let mut transaction = app.repository.begin().await.unwrap();
    let token = app
        .repository
        .get_password_reset_token(&mut transaction, user_id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    token
}

pub async fn list_auth_methods_response(client: &Client, app: &TestedApp) -> reqwest::Response {
    client
        .get(format!("{}users/me/auth-methods", app.api_address))
        .send()
        .await
        .unwrap()
}

pub async fn list_auth_methods(client: &Client, app: &TestedApp) -> Vec<UserAuthMethod> {
    list_auth_methods_response(client, app)
        .await
        .json()
        .await
        .unwrap()
}

pub async fn add_local_auth_response(
    client: &Client,
    app: &TestedApp,
    password: &str,
) -> reqwest::Response {
    client
        .post(format!("{}users/me/auth-methods/local", app.api_address))
        .header(reqwest::header::ORIGIN, front_origin_header(app))
        .json(&SecretBox::new(Box::new(Password(password.to_string()))))
        .send()
        .await
        .unwrap()
}

pub async fn remove_auth_method_response(
    client: &Client,
    app: &TestedApp,
    kind: UserAuthKind,
) -> reqwest::Response {
    client
        .delete(format!("{}users/me/auth-methods/{kind}", app.api_address))
        .header(reqwest::header::ORIGIN, front_origin_header(app))
        .send()
        .await
        .unwrap()
}

pub async fn create_user(app: &TestedApp, email: EmailAddress, password: &str) -> User {
    let service = app.user_service.clone();
    let mut transaction = app.repository.begin().await.unwrap();
    let new_user = app
        .repository
        .create_user(
            &mut transaction,
            User::new(None, None, email),
            UserAuth::Local(Box::new(LocalUserAuth {
                password_hash: service
                    .get_new_password_hash(SecretBox::new(Box::new(password.parse().unwrap())))
                    .unwrap(),
                password_reset_at: None,
                password_reset_sent_at: None,
            })),
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    new_user
}

pub async fn create_user_and_login(
    app: &TestedApp,
    email: EmailAddress,
    password: &str,
) -> (Client, User) {
    let user = create_user(app, email.clone(), password).await;
    let client = Client::builder().cookie_store(true).build().unwrap();
    let login_response = login_user_response(&client, app, email, password).await;
    assert_eq!(login_response.status(), http::StatusCode::OK);
    (client, user)
}

pub async fn patch_user_response(
    client: &Client,
    app: &TestedApp,
    patch: &UserPatch,
) -> reqwest::Response {
    client
        .patch(format!("{}users/me", app.api_address))
        .json(patch)
        .send()
        .await
        .unwrap()
}

pub async fn start_add_passkey_registration_response(
    client: &Client,
    app: &TestedApp,
    username: &str,
) -> reqwest::Response {
    client
        .post(format!(
            "{}users/me/auth-methods/passkey/registration/start",
            app.api_address
        ))
        .header(reqwest::header::ORIGIN, front_origin_header(app))
        .json(&Username(username.to_string()))
        .send()
        .await
        .unwrap()
}

pub async fn finish_add_passkey_registration_response(
    client: &Client,
    app: &TestedApp,
    register_credentials: &RegisterPublicKeyCredential,
    nonce: &str,
) -> reqwest::Response {
    client
        .post(format!(
            "{}users/me/auth-methods/passkey/registration/finish",
            app.api_address
        ))
        .header(reqwest::header::ORIGIN, front_origin_header(app))
        .json(&finish_body_with_nonce(register_credentials, nonce))
        .send()
        .await
        .unwrap()
}

pub async fn link_oidc_pkce_session_response(
    client: &Client,
    app: &TestedApp,
    params: &SessionAuthValidationParameters,
) -> reqwest::Response {
    client
        .post(format!("{}auth/link-oidc/session", app.api_address))
        .json(params)
        .send()
        .await
        .unwrap()
}

pub async fn start_passkey_registration_response(
    client: &Client,
    app: &TestedApp,
    username: &str,
) -> reqwest::Response {
    client
        .post(format!(
            "{}users/passkeys/registration/start",
            app.api_address
        ))
        .header(reqwest::header::ORIGIN, front_origin_header(app))
        .json(&Username(username.to_string()))
        .send()
        .await
        .unwrap()
}

pub async fn start_passkey_authentication_response(
    client: &Client,
    app: &TestedApp,
    username: &str,
) -> reqwest::Response {
    client
        .post(format!(
            "{}users/passkeys/authentication/start",
            app.api_address
        ))
        .header(reqwest::header::ORIGIN, front_origin_header(app))
        .json(&Username(username.to_string()))
        .send()
        .await
        .unwrap()
}

pub async fn finish_passkey_registration_response_unauthenticated(
    client: &Client,
    app: &TestedApp,
    register_credentials: &RegisterPublicKeyCredential,
    nonce: &str,
) -> reqwest::Response {
    client
        .post(format!(
            "{}users/passkeys/registration/finish",
            app.api_address
        ))
        .header(reqwest::header::ORIGIN, front_origin_header(app))
        .json(&finish_body_with_nonce(register_credentials, nonce))
        .send()
        .await
        .unwrap()
}
