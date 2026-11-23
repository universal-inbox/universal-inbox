use std::{collections::HashMap, time::SystemTime};

use cucumber::{given, then, when};
use email_address::EmailAddress;
use openidconnect::AccessToken;
use pretty_assertions::assert_eq;
use reqwest::Client;

use universal_inbox::{
    SuccessResponse, auth::SessionAuthValidationParameters, pii::Pii, user::User,
};
use universal_inbox_api::mailer::EmailTemplate;

use crate::{
    helpers::{
        auth::fetch_auth_tokens_for_user,
        user::{
            create_user_with_legacy_password, get_current_user, get_current_user_response,
            get_user_email_validation_token, login_user_response, register_user,
            register_user_response,
        },
    },
    steps::id_tokens::signed_id_token,
    world::{ApiWorld, Response, Session},
};

fn email_of(email: &str) -> Pii<EmailAddress> {
    email
        .parse()
        .unwrap_or_else(|err| panic!("{email:?} is not an email address: {err}"))
}

#[when(expr = "I sign up with the email {string} and the password {string}")]
async fn i_sign_up(world: &mut ApiWorld, email: String, password: String) {
    let email = email_of(&email);
    let app = world.app.get().await;
    let (client, user) = register_user(app, email.clone(), &password).await;
    world.account.set(email, &password, user.clone());
    world.session = Some(Session { client, user });
}

#[when(expr = "I sign up again with the email {string} and the password {string}")]
async fn i_sign_up_again(world: &mut ApiWorld, email: String, password: String) {
    let client = world.client();
    let app = world.app.get().await;
    let response = register_user_response(&client, app, email_of(&email), &password).await;
    world.http.record(Response::read(response).await);
}

#[when(expr = "a visitor signs up with the email {string} and the password {string}")]
async fn a_visitor_signs_up(world: &mut ApiWorld, email: String, password: String) {
    let client = world.browser.client().clone();
    let app = world.app.get().await;
    let response = register_user_response(&client, app, email_of(&email), &password).await;
    world.http.record(Response::read(response).await);
}

#[when(expr = "a visitor signs in with the email {string} and the password {string}")]
async fn a_visitor_signs_in(world: &mut ApiWorld, email: String, password: String) {
    let app = world.app.get().await;
    let response = login_user_response(&Client::new(), app, email_of(&email), &password).await;
    world.http.record(Response::read(response).await);
}

#[given(expr = "an account was created with the old password {string} and the email {string}")]
async fn an_account_was_created_with_an_old_password(
    world: &mut ApiWorld,
    password: String,
    email: String,
) {
    let email = email_of(&email);
    let app = world.app.get().await;
    let user = create_user_with_legacy_password(app, email.clone(), &password).await;
    world.account.set(email, &password, user);
}

#[when(expr = "I sign in with a fresh identity token for {string}")]
async fn i_sign_in_with_a_fresh_identity_token(world: &mut ApiWorld, email: String) {
    let url = format!("{}auth/session", world.app.get().await.api_address);
    let issuer = world
        .app
        .get()
        .await
        .oidc_issuer_mock_server
        .as_ref()
        .unwrap()
        .uri();
    let response = world
        .browser
        .client()
        .post(url)
        .json(&SessionAuthValidationParameters {
            auth_id_token: signed_id_token(issuer, &email).into(),
            access_token: AccessToken::new("fake_token".to_string()),
        })
        .send()
        .await
        .unwrap();
    world.http.record(Response::read(response).await);
}

#[when("I look at my account")]
async fn i_look_at_my_account(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let response = get_current_user_response(&client, app).await;
    world.http.record(Response::read(response).await);
}

#[when("I look at my account from a new browser")]
async fn i_look_at_my_account_from_a_new_browser(world: &mut ApiWorld) {
    let client = world.browser.client().clone();
    let app = world.app.get().await;
    let response = get_current_user_response(&client, app).await;
    world.http.record(Response::read(response).await);
}

#[then(expr = "the app says {string}")]
fn the_app_says(world: &mut ApiWorld, message: String) {
    let body: SuccessResponse = world.http.last().json();
    assert_eq!(body.message, message);
}

#[then("both answers carry the same message")]
fn both_answers_carry_the_same_message(world: &mut ApiWorld) {
    let messages: Vec<String> = world
        .http
        .all()
        .iter()
        .map(|response| response.json::<SuccessResponse>().message)
        .collect();
    assert_eq!(messages[0], messages[1]);
}

#[then(expr = "the error message is {string}")]
fn the_error_message_is(world: &mut ApiWorld, message: String) {
    let body: HashMap<String, String> = world.http.last().json();
    assert_eq!(body.get("message").unwrap(), &message);
}

#[then(expr = "my account uses the email {string}")]
fn my_account_uses_the_email(world: &mut ApiWorld, email: String) {
    assert_eq!(world.user().email, Some(email_of(&email)));
}

#[then("my email address is not verified yet and a verification email was sent")]
fn my_email_address_is_not_verified_yet(world: &mut ApiWorld) {
    let user = world.user();
    assert!(user.email_validated_at.is_none());
    assert!(!user.is_email_validated());
    assert!(user.email_validation_sent_at.is_some());
}

#[then("I have no API tokens")]
async fn i_have_no_api_tokens(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    assert_eq!(fetch_auth_tokens_for_user(app, user_id).await.len(), 0);
}

#[then("a verification token was issued for my account")]
async fn a_verification_token_was_issued(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    assert!(
        get_user_email_validation_token(app, user_id)
            .await
            .is_some()
    );
}

#[then(expr = "{int} email(s) was/were sent in total")]
async fn emails_were_sent_in_total(world: &mut ApiWorld, expected: usize) {
    let app = world.app.get().await;
    let emails_sent = app.mailer_stub.read().await.emails_sent.read().await.len();
    assert_eq!(emails_sent, expected);
}

#[then("the latest email sent is the verification email with my current link")]
async fn the_latest_email_is_the_verification_email(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let token = get_user_email_validation_token(app, user_id)
        .await
        .expect("The account has no verification token");
    let emails_sent = (*app.mailer_stub.read().await.emails_sent.read().await).clone();
    let (recipient, template) = emails_sent.last().expect("No email was sent");
    assert_eq!(recipient.id, user_id);
    assert_eq!(
        *template,
        EmailTemplate::EmailVerification {
            first_name: None,
            email_verification_url: format!(
                "{}users/{}/email-verification/{}",
                app.front_base_url, user_id, token
            )
            .parse()
            .unwrap()
        }
    );
}

#[then("the owner of the account was told about the registration attempt")]
async fn the_owner_was_told_about_the_registration_attempt(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let emails_sent = (*app.mailer_stub.read().await.emails_sent.read().await).clone();
    let (_, template) = emails_sent
        .iter()
        .find(|(_, template)| {
            matches!(
                template,
                EmailTemplate::RegistrationAttemptOnExistingAccount { .. }
            )
        })
        .expect("Registration attempt email should have been sent");
    assert_eq!(
        *template,
        EmailTemplate::RegistrationAttemptOnExistingAccount {
            first_name: None,
            login_url: format!("{}login", app.front_base_url).parse().unwrap(),
            password_reset_url: format!("{}password-reset", app.front_base_url)
                .parse()
                .unwrap(),
        }
    );
}

#[then("any session cookie it sets is cleared")]
async fn any_session_cookie_it_sets_is_cleared(world: &mut ApiWorld) {
    let cookie_name = world.app.get().await.session_cookie_name();
    for cookie in &world.http.last().cookies {
        assert_eq!(cookie.name, cookie_name);
        assert_eq!(cookie.value, "");
        assert!(cookie.expires.unwrap() < SystemTime::now());
    }
}

#[then(
    "it sets a session cookie that is secure, HTTP-only, same-site lax, valid for the whole site and not tied to a domain"
)]
async fn it_sets_a_hardened_session_cookie(world: &mut ApiWorld) {
    let cookie_name = world.app.get().await.session_cookie_name();
    let cookie = world
        .http
        .last()
        .cookies
        .iter()
        .find(|cookie| cookie.name == cookie_name)
        .expect("the login sets the session cookie");
    assert!(cookie.secure);
    assert!(cookie.http_only);
    assert!(cookie.same_site_lax);
    assert_eq!(cookie.path.as_deref(), Some("/"));
    assert_eq!(cookie.domain, None);
}

#[then("the response is my account")]
fn the_response_is_my_account(world: &mut ApiWorld) {
    let logged_user: User = world.http.last().json();
    assert_eq!(logged_user.id, world.account.user().id);
}

#[then(expr = "from that browser my account uses the email {string} and is not verified yet")]
async fn from_that_browser_my_account_is_not_verified(world: &mut ApiWorld, email: String) {
    let client = world.browser.client().clone();
    let app = world.app.get().await;
    let user = get_current_user(&client, app).await;
    assert_eq!(user.email, Some(email_of(&email)));
    assert!(user.email_validated_at.is_none());
    assert!(user.email_validation_sent_at.is_some());
}
