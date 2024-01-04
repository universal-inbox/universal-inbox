use email_address::EmailAddress;
use reqwest::Client;
use secrecy::Secret;

use universal_inbox::user::{
    Credentials, EmailValidationToken, Password, PasswordResetToken, RegisterUserParameters, User,
    UserId,
};

use universal_inbox_api::repository::user::UserRepository;

use crate::helpers::TestedApp;

pub async fn register_user_response(
    client: &Client,
    app: &TestedApp,
    first_name: &str,
    last_name: &str,
    email: EmailAddress,
    password: &str,
) -> reqwest::Response {
    client
        .post(&format!("{}users", app.api_address))
        .json(&RegisterUserParameters {
            first_name: first_name.to_string(),
            last_name: last_name.to_string(),
            credentials: Credentials {
                email,
                password: Secret::new(Password(password.to_string())),
            },
        })
        .send()
        .await
        .unwrap()
}

pub async fn register_user(
    app: &TestedApp,
    first_name: &str,
    last_name: &str,
    email: EmailAddress,
    password: &str,
) -> (Client, User) {
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .build()
        .unwrap();

    let response =
        register_user_response(&client, app, first_name, last_name, email, password).await;

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
        .get(&format!("{}users/me", app.api_address))
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
        .post(&format!("{}users/me", app.api_address))
        .json(&Credentials {
            email,
            password: Secret::new(Password(password.to_string())),
        })
        .send()
        .await
        .unwrap()
}

pub async fn logout_user_response(client: &Client, api_address: &str) -> reqwest::Response {
    client
        .delete(&format!("{api_address}auth/session"))
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
