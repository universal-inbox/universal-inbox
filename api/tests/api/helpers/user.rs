use email_address::EmailAddress;
use reqwest::Client;
use secrecy::SecretBox;

use universal_inbox::user::{
    Credentials, EmailValidationToken, Password, PasswordResetToken, RegisterUserParameters, User,
    UserAuthKind, UserAuthMethod, UserId, UserPatch,
};

use universal_inbox_api::{
    repository::user::UserRepository,
    universal_inbox::user::model::{LocalUserAuth, UserAuth},
};

use crate::helpers::TestedApp;

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
