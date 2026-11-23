use std::{str::FromStr, time::Duration};

use chrono::Utc;
use cucumber::{then, when};
use pretty_assertions::assert_eq;
use reqwest::{Client, StatusCode, header::ORIGIN};
use secrecy::SecretBox;

use uuid::Uuid;

use universal_inbox::user::{Password, PasswordChange, PasswordResetToken, UserAuthKind, UserId};
use universal_inbox_api::{mailer::EmailTemplate, universal_inbox::user::model::UserAuth};

use crate::{
    helpers::{
        TestedApp,
        auth::get_user_auth,
        user::{
            emails_sent_to, front_origin_header, get_current_user_response,
            get_password_reset_token, login_user_response, reset_password_response,
        },
    },
    world::{ApiWorld, Response},
};

fn password_change(current_password: &str, new_password: &str) -> PasswordChange {
    PasswordChange {
        current_password: SecretBox::new(Box::new(Password(current_password.to_string()))),
        new_password: SecretBox::new(Box::new(Password(new_password.to_string()))),
    }
}

async fn change_password(
    client: &Client,
    app: &TestedApp,
    current_password: &str,
    new_password: &str,
    saying_where_it_comes_from: bool,
) -> reqwest::Response {
    let mut request = client
        .patch(format!("{}users/me/auth-methods/local", app.api_address))
        .json(&password_change(current_password, new_password));
    if saying_where_it_comes_from {
        request = request.header(ORIGIN, front_origin_header(app));
    }
    request.send().await.unwrap()
}

/// Session revocation has the one-second precision of the JWT `iat` claim: a session issued in
/// the same second as a password change survives it. Waiting for the next second makes the
/// sessions opened before the change revoked.
#[when("I wait for the next second")]
async fn i_wait_for_the_next_second(_world: &mut ApiWorld) {
    let millis_into_second = Utc::now().timestamp_subsec_millis() as u64;
    tokio::time::sleep(Duration::from_millis(1_001 - millis_into_second)).await;
}

#[when(expr = "I change my password from {string} to {string}")]
async fn i_change_my_password(
    world: &mut ApiWorld,
    current_password: String,
    new_password: String,
) {
    let client = world.client();
    let app = world.app.get().await;
    let response = change_password(&client, app, &current_password, &new_password, true).await;
    world.http.record(Response::read(response).await);
}

#[when(
    expr = "I change my password from {string} to {string} in a request that does not say which website it comes from"
)]
async fn i_change_my_password_without_an_origin(
    world: &mut ApiWorld,
    current_password: String,
    new_password: String,
) {
    let client = world.client();
    let app = world.app.get().await;
    let response = change_password(&client, app, &current_password, &new_password, false).await;
    world.http.record(Response::read(response).await);
}

#[when(expr = "a visitor who is not signed in changes the password from {string} to {string}")]
async fn a_visitor_changes_the_password(
    world: &mut ApiWorld,
    current_password: String,
    new_password: String,
) {
    let client = world.browser.client().clone();
    let app = world.app.get().await;
    let response = change_password(&client, app, &current_password, &new_password, true).await;
    world.http.record(Response::read(response).await);
}

#[when(
    expr = "I change my password from {string} to {string} {int} times, each time refused as invalid"
)]
async fn i_change_my_password_several_times(
    world: &mut ApiWorld,
    current_password: String,
    new_password: String,
    attempts: usize,
) {
    let client = world.client();
    let app = world.app.get().await;
    for attempt in 1..=attempts {
        let response = change_password(&client, app, &current_password, &new_password, true).await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "attempt {attempt} should be rejected as invalid input"
        );
    }
}

#[then("I am still signed in")]
async fn i_am_still_signed_in(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let response = get_current_user_response(&client, app).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[when(expr = "I sign in from a new browser with the password {string}")]
async fn i_sign_in_from_a_new_browser(world: &mut ApiWorld, password: String) {
    let email = world.account.email();
    let client = world.browser.client().clone();
    let app = world.app.get().await;
    let response = login_user_response(&client, app, email, &password).await;
    world.http.record(Response::read(response).await);
}

#[then("that new browser is signed in")]
async fn that_new_browser_is_signed_in(world: &mut ApiWorld) {
    let client = world.browser.client().clone();
    let app = world.app.get().await;
    let response = get_current_user_response(&client, app).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[then(expr = "I received {int} password changed email(s)")]
async fn i_received_password_changed_emails(world: &mut ApiWorld, expected: usize) {
    let email = world.account.email();
    let app = world.app.get().await;
    let count = app
        .mailer_stub
        .read()
        .await
        .emails_sent
        .read()
        .await
        .iter()
        .filter(|(user, template)| {
            user.email.as_ref() == Some(&email)
                && matches!(template, EmailTemplate::PasswordChanged { .. })
        })
        .count();
    assert_eq!(count, expected);
}

#[then(expr = "the response text contains {string}")]
fn the_response_text_contains(world: &mut ApiWorld, expected: String) {
    let text = world.http.last().text();
    assert!(
        text.contains(&expected),
        "{text:?} does not contain {expected:?}"
    );
}

#[then(expr = "the response has a {string} header")]
fn the_response_has_a_header(world: &mut ApiWorld, name: String) {
    assert!(
        world.http.last().headers.contains_key(&name),
        "No {name} header in {:?}",
        world.http.last().headers
    );
}

#[when("I ask for my password to be reset")]
async fn i_ask_for_my_password_to_be_reset(world: &mut ApiWorld) {
    let email = world.account.email();
    let client = world.browser.client().clone();
    let url = format!("{}users/password-reset", world.app.get().await.api_address);
    let response = Response::read(client.post(url).json(&email).send().await.unwrap()).await;
    let sent = response.status == StatusCode::OK;
    world.http.record(response);
    if sent {
        let user_id = world.account.user().id;
        let app = world.app.get().await;
        world.reset.token = get_password_reset_token(app, user_id).await;
    }
}

#[when(expr = "I reset my password to {string} with the link in the email")]
async fn i_reset_my_password(world: &mut ApiWorld, new_password: String) {
    let user_id = world.account.user().id;
    let client = world.browser.client().clone();
    let token = match world.reset.token.take() {
        Some(token) => token,
        None => {
            let app = world.app.get().await;
            get_password_reset_token(app, user_id)
                .await
                .expect("No password reset email was sent")
        }
    };
    let app = world.app.get().await;
    let response = reset_password_response(&client, app, user_id, token, &new_password).await;
    world.http.record(Response::read(response).await);
}

#[then("the latest email sent is the password reset email with my link")]
async fn the_latest_email_is_the_password_reset_email(world: &mut ApiWorld) {
    let user_id = world.account.user().id;
    let token = world
        .reset
        .token
        .clone()
        .expect("No password reset email was asked for");
    let app = world.app.get().await;
    let emails_sent = (*app.mailer_stub.read().await.emails_sent.read().await).clone();
    let (recipient, template) = emails_sent.last().expect("No email was sent");
    assert_eq!(recipient.id, user_id);
    assert_eq!(
        *template,
        EmailTemplate::PasswordReset {
            first_name: None,
            password_reset_url: format!(
                "{}users/{}/password-reset/{token}",
                app.front_base_url, user_id
            )
            .parse()
            .unwrap()
        }
    );
}

/// The emailed web address carries the token to the reset page, which sends it to the API in the
/// request body, never in the API address.
#[when(expr = "I send the new password {string} to the API address that carries the reset token")]
async fn i_send_the_new_password_to_an_address_with_the_token(
    world: &mut ApiWorld,
    new_password: String,
) {
    let user_id = world.account.user().id;
    let token = world
        .reset
        .token
        .clone()
        .expect("No password reset email was asked for");
    let client = world.browser.client().clone();
    let url = format!(
        "{}users/{user_id}/password-reset/{token}",
        world.app.get().await.api_address
    );
    let response = client
        .post(url)
        .json(&Password::from_str(&new_password).unwrap())
        .send()
        .await
        .unwrap();
    world.http.record(Response::read(response).await);
}

#[then("my password reset token is used up")]
async fn my_password_reset_token_is_used_up(world: &mut ApiWorld) {
    let user_id = world.account.user().id;
    let app = world.app.get().await;
    assert!(get_password_reset_token(app, user_id).await.is_none());
}

#[then("no password reset token exists for my account")]
async fn no_password_reset_token_exists(world: &mut ApiWorld) {
    my_password_reset_token_is_used_up(world).await;
}

#[then("my latest email is a password changed email")]
async fn my_latest_email_is_a_password_changed_email(world: &mut ApiWorld) {
    let email = world.account.email();
    let app = world.app.get().await;
    let emails = emails_sent_to(app, &email).await;
    assert!(
        matches!(emails.last(), Some(EmailTemplate::PasswordChanged { .. })),
        "{emails:?}"
    );
}

#[then("my account records when the password reset was sent and done")]
async fn my_account_records_the_password_reset(world: &mut ApiWorld) {
    let user_id = world.account.user().id;
    let app = world.app.get().await;
    let UserAuth::Local(local_user_auth) = get_user_auth(app, user_id, UserAuthKind::Local).await
    else {
        panic!("User should have local auth");
    };
    assert!(local_user_auth.password_reset_at.is_some());
    assert!(local_user_auth.password_reset_sent_at.is_some());
}

#[when(
    expr = "I reset my password to {string} with the link in the email but with the id of another user"
)]
async fn i_reset_my_password_with_another_users_id(world: &mut ApiWorld, new_password: String) {
    let other_user_id = UserId(Uuid::new_v4());
    world.reset.link_user_id = Some(other_user_id);
    let token = world
        .reset
        .token
        .take()
        .expect("No password reset email was asked for");
    let client = world.browser.client().clone();
    let app = world.app.get().await;
    let response = reset_password_response(&client, app, other_user_id, token, &new_password).await;
    world.http.record(Response::read(response).await);
}

#[when(expr = "I reset my password to {string} with a token I was never sent")]
async fn i_reset_my_password_with_an_unknown_token(world: &mut ApiWorld, new_password: String) {
    let user_id = world.user().id;
    world.reset.link_user_id = Some(user_id);
    let client = world.browser.client().clone();
    let app = world.app.get().await;
    let response = reset_password_response(
        &client,
        app,
        user_id,
        PasswordResetToken(Uuid::new_v4()),
        &new_password,
    )
    .await;
    world.http.record(Response::read(response).await);
}

#[then("the error message says the password reset token is invalid for the user in the link")]
fn the_error_says_the_reset_token_is_invalid(world: &mut ApiWorld) {
    let user_id = world
        .reset
        .link_user_id
        .expect("No password reset was tried");
    let body: std::collections::HashMap<String, String> = world.http.last().json();
    assert_eq!(
        body.get("message").unwrap(),
        &format!("Invalid input data: Invalid password reset token for user {user_id}")
    );
}

#[when("two hours pass after my password reset email was sent")]
async fn my_password_reset_email_was_sent_two_hours_ago(world: &mut ApiWorld) {
    let user_id = world.account.user().id;
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    sqlx::query(
        "UPDATE user_auth SET password_reset_sent_at = password_reset_sent_at - interval '2 hours' WHERE user_id = $1",
    )
    .bind(user_id.0)
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
}
