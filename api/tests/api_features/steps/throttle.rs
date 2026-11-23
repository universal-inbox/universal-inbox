use cucumber::{given, then, when};
use email_address::EmailAddress;
use pretty_assertions::assert_eq;
use reqwest::{Client, StatusCode};
use secrecy::SecretBox;
use uuid::Uuid;

use universal_inbox::{
    pii::Pii,
    user::{Credentials, Password, RegisterUserParameters},
};
use universal_inbox_api::{configuration::LocalAuthenticationSettings, mailer::EmailTemplate};

use crate::{
    helpers::{TestedApp, local_auth_settings, user::emails_sent_to},
    state::Observed,
    world::{ApiWorld, Response},
};

const PASSWORD: &str = "Very-harD-pasSword-5";

fn unique_email() -> Pii<EmailAddress> {
    format!("throttle-{}@example.com", Uuid::new_v4())
        .parse()
        .unwrap()
}

#[given(
    expr = "each account may make {int} login requests and each email address may receive {int} account emails in a period"
)]
fn each_account_has_a_budget(world: &mut ApiWorld, login_requests: u32, emails: u32) {
    world.app.configure().local_auth_policy = Some(LocalAuthenticationSettings {
        max_login_requests_per_account: login_requests,
        max_account_emails_per_address: emails,
        ..local_auth_settings()
    });
}

#[given("I have no account")]
fn i_have_no_account(world: &mut ApiWorld) {
    world.account.set_without_account(unique_email(), PASSWORD);
}

#[given("an email address that no account uses")]
fn an_email_address_that_no_account_uses(world: &mut ApiWorld) {
    world.account.set_unregistered(unique_email());
}

async fn observe(response: reqwest::Response) -> Observed {
    Observed {
        status: response.status(),
        has_retry_after: response
            .headers()
            .contains_key(reqwest::header::RETRY_AFTER),
        body: response.text().await.unwrap(),
    }
}

#[derive(Clone, Copy)]
enum AccountEmail {
    PasswordReset,
    Registration,
}

async fn request_account_email(
    app: &TestedApp,
    kind: AccountEmail,
    email: &Pii<EmailAddress>,
    address: String,
) -> reqwest::Response {
    let client = Client::new();
    match kind {
        AccountEmail::PasswordReset => client
            .post(format!("{}users/password-reset", app.api_address))
            .header("X-Forwarded-For", address)
            .json(email),
        AccountEmail::Registration => client
            .post(format!("{}users", app.api_address))
            .header("X-Forwarded-For", address)
            .json(&RegisterUserParameters {
                credentials: Credentials {
                    email: email.clone(),
                    password: SecretBox::new(Box::new(Password(PASSWORD.to_string()))),
                },
            }),
    }
    .send()
    .await
    .unwrap()
}

async fn sign_in_from(
    app: &TestedApp,
    email: &Pii<EmailAddress>,
    address: String,
) -> reqwest::Response {
    Client::builder()
        .cookie_store(true)
        .build()
        .unwrap()
        .post(format!("{}users/me", app.api_address))
        .header("X-Forwarded-For", address)
        .json(&Credentials {
            email: email.clone(),
            password: SecretBox::new(Box::new(Password(PASSWORD.to_string()))),
        })
        .send()
        .await
        .unwrap()
}

async fn resend_verification_email_from(
    client: &Client,
    app: &TestedApp,
    address: String,
) -> reqwest::Response {
    client
        .post(format!("{}users/me/email-verification", app.api_address))
        .header("X-Forwarded-For", address)
        .send()
        .await
        .unwrap()
}

async fn series(world: &mut ApiWorld, kind: AccountEmail, count: usize) {
    let emails = [world.account.email(), world.account.unregistered()];
    let app = world.app.get().await;
    let mut all = Vec::new();
    for email in &emails {
        let mut observed = Vec::new();
        world.probe.next = 0;
        for _ in 0..count {
            let address = world.probe.next_address();
            let response = request_account_email(app, kind, email, address).await;
            observed.push(observe(response).await);
        }
        all.push(observed);
    }
    world.probe.series = all;
}

#[when(
    expr = "I request {int} password resets for each of the registered and the unregistered email addresses, each from a different network address"
)]
async fn i_request_password_resets_for_both_addresses(world: &mut ApiWorld, count: usize) {
    series(world, AccountEmail::PasswordReset, count).await;
}

#[when(
    expr = "I request {int} registrations for each of the registered and the unregistered email addresses, each from a different network address"
)]
async fn i_request_registrations_for_both_addresses(world: &mut ApiWorld, count: usize) {
    series(world, AccountEmail::Registration, count).await;
}

#[then(
    expr = "for both email addresses the first {int} requests were answered 200 and the next one was throttled with a Retry-After header"
)]
fn the_first_requests_succeeded_and_the_next_was_throttled(world: &mut ApiWorld, allowed: usize) {
    for observed in &world.probe.series {
        for answer in &observed[..allowed] {
            assert_eq!(answer.status, StatusCode::OK);
        }
        let last = observed.last().unwrap();
        assert_eq!(last.status, StatusCode::TOO_MANY_REQUESTS);
        assert!(
            last.has_retry_after,
            "429 response must carry a Retry-After header"
        );
    }
}

#[then("both email addresses got exactly the same responses")]
fn both_email_addresses_got_the_same_responses(world: &mut ApiWorld) {
    assert_eq!(
        world.probe.series[0], world.probe.series[1],
        "unregistered emails must get exactly the same responses as registered ones"
    );
}

#[then(expr = "the registered email address received {int} emails")]
async fn the_registered_email_address_received_emails(world: &mut ApiWorld, expected: usize) {
    let email = world.account.email();
    let app = world.app.get().await;
    assert_eq!(emails_sent_to(app, &email).await.len(), expected);
}

#[then(expr = "I received {int} account lockout email(s)")]
async fn i_received_account_lockout_emails(world: &mut ApiWorld, expected: usize) {
    let app = world.app.get().await;
    let emails_sent = (*app.mailer_stub.read().await.emails_sent.read().await).clone();
    let count = emails_sent
        .iter()
        .filter(|(_, template)| matches!(template, EmailTemplate::AccountLockout { .. }))
        .count();
    assert_eq!(count, expected);
}

#[when(expr = "I sign in {int} times with the password {string}, each time answered {int}")]
async fn i_sign_in_several_times(
    world: &mut ApiWorld,
    attempts: usize,
    password: String,
    status: u16,
) {
    let email = world.account.email();
    let client = world.browser.client().clone();
    let app = world.app.get().await;
    for attempt in 1..=attempts {
        let response =
            crate::helpers::user::login_user_response(&client, app, email.clone(), &password).await;
        assert_eq!(
            response.status(),
            status,
            "attempt {attempt} for {password:?} should be answered {status}"
        );
    }
}

#[when(
    expr = "I alternate {int} times between registering the registered email address and asking for its password to be reset, each from a different network address and each answered 200"
)]
async fn i_alternate_registering_and_resetting(world: &mut ApiWorld, count: u32) {
    let email = world.account.email();
    for n in 0..count {
        let kind = if n % 2 == 0 {
            AccountEmail::Registration
        } else {
            AccountEmail::PasswordReset
        };
        let address = world.probe.next_address();
        let app = world.app.get().await;
        let response = request_account_email(app, kind, &email, address).await;
        assert_eq!(response.status(), StatusCode::OK);
    }
}

#[when(
    expr = "I alternate {int} times between asking for the verification email again and asking for a password reset, each from a different network address and each answered 200"
)]
async fn i_alternate_resending_and_resetting(world: &mut ApiWorld, count: u32) {
    let email = world.account.email();
    let client = world.client();
    for n in 0..count {
        let address = world.probe.next_address();
        let app = world.app.get().await;
        let response = if n % 2 == 0 {
            resend_verification_email_from(&client, app, address).await
        } else {
            request_account_email(app, AccountEmail::PasswordReset, &email, address).await
        };
        assert_eq!(response.status(), StatusCode::OK);
    }
}

#[when("I ask for a password reset for the registered email address from a new network address")]
async fn i_ask_for_a_password_reset_from_a_new_address(world: &mut ApiWorld) {
    let email = world.account.email();
    let address = world.probe.next_address();
    let app = world.app.get().await;
    let response = request_account_email(app, AccountEmail::PasswordReset, &email, address).await;
    world.http.record(Response::read(response).await);
}

#[when("I register the registered email address again from a new network address")]
async fn i_register_again_from_a_new_address(world: &mut ApiWorld) {
    let email = world.account.email();
    let address = world.probe.next_address();
    let app = world.app.get().await;
    let response = request_account_email(app, AccountEmail::Registration, &email, address).await;
    world.http.record(Response::read(response).await);
}

#[when(
    expr = "I sign in with my password {int} times, each from a different network address and each answered 200"
)]
async fn i_sign_in_from_different_addresses(world: &mut ApiWorld, count: u32) {
    let email = world.account.email();
    for n in 0..count {
        let address = world.probe.next_address();
        let app = world.app.get().await;
        let response = sign_in_from(app, &email, address).await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "login {n} should succeed"
        );
    }
}

#[when("I sign in with my password from a new network address")]
async fn i_sign_in_from_a_new_address(world: &mut ApiWorld) {
    let email = world.account.email();
    let address = world.probe.next_address();
    let app = world.app.get().await;
    let response = sign_in_from(app, &email, address).await;
    world.http.record(Response::read(response).await);
}

#[when(
    expr = "I ask {int} times for the verification email to be sent again, each from a different network address and each answered 200"
)]
async fn i_ask_for_the_verification_email_again(world: &mut ApiWorld, count: u32) {
    let client = world.client();
    for n in 0..count {
        let address = world.probe.next_address();
        let app = world.app.get().await;
        let response = resend_verification_email_from(&client, app, address).await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "resend {n} should succeed"
        );
    }
}

#[when("I ask for the verification email to be sent again from a new network address")]
async fn i_ask_for_the_verification_email_from_a_new_address(world: &mut ApiWorld) {
    let client = world.client();
    let address = world.probe.next_address();
    let app = world.app.get().await;
    let response = resend_verification_email_from(&client, app, address).await;
    world.http.record(Response::read(response).await);
}
