use std::time::Duration;

use chrono::Utc;
use cucumber::{then, when};
use pretty_assertions::assert_eq;
use reqwest::{Client, StatusCode, header::ORIGIN};
use secrecy::SecretBox;

use universal_inbox::user::{Password, PasswordChange};
use universal_inbox_api::mailer::EmailTemplate;

use crate::{
    helpers::{
        TestedApp,
        user::{
            front_origin_header, get_current_user_response, get_password_reset_token,
            login_user_response, reset_password_response,
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
    let response = client.post(url).json(&email).send().await.unwrap();
    world.http.record(Response::read(response).await);
}

#[when(expr = "I reset my password to {string} with the link in the email")]
async fn i_reset_my_password(world: &mut ApiWorld, new_password: String) {
    let user_id = world.account.user().id;
    let client = world.browser.client().clone();
    let app = world.app.get().await;
    let token = get_password_reset_token(app, user_id)
        .await
        .expect("No password reset email was sent");
    let response = reset_password_response(&client, app, user_id, token, &new_password).await;
    world.http.record(Response::read(response).await);
}
