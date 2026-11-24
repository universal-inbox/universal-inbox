use std::collections::HashMap;

use cucumber::{gherkin::Step, given, then, when};
use email_address::EmailAddress;
use pretty_assertions::assert_eq;
use reqwest::StatusCode;

use universal_inbox::{
    FrontConfig,
    pii::Pii,
    user::{USER_NAME_MAX_LENGTH, User, UserPatch},
};
use universal_inbox_api::mailer::EmailTemplate;

use crate::{
    helpers::user::{
        emails_sent_to, get_current_user, get_pending_email_change_token, patch_user_response,
        register_user,
    },
    world::{ApiWorld, Response},
};

const PASSWORD: &str = "Very-harD-pasSword-5";

fn email_of(email: &str) -> Pii<EmailAddress> {
    email
        .parse()
        .unwrap_or_else(|err| panic!("{email:?} is not an email address: {err}"))
}

#[given("the app does not send emails")]
fn the_app_does_not_send_emails(world: &mut ApiWorld) {
    world.app.configure().settings.application.email = None;
}

#[given("the app only lets users sign in with single sign-on")]
fn the_app_only_lets_users_sign_in_with_single_sign_on(world: &mut ApiWorld) {
    let config = world.app.configure();
    config.identity_provider = true;
    config.local_auth = None;
}

#[given(expr = "another user signed up with the email {string}")]
async fn another_user_signed_up(world: &mut ApiWorld, email: String) {
    let app = world.app.get().await;
    register_user(app, email_of(&email), PASSWORD).await;
}

async fn patch(world: &mut ApiWorld, patch: UserPatch) {
    let client = world.client();
    let app = world.app.get().await;
    let response = patch_user_response(&client, app, &patch).await;
    world.http.record(Response::read(response).await);
}

#[when(expr = "I change my first name to {string} and my last name to {string}")]
async fn i_change_my_names(world: &mut ApiWorld, first_name: String, last_name: String) {
    patch(
        world,
        UserPatch {
            first_name: Some(Pii::new(first_name)),
            last_name: Some(Pii::new(last_name)),
            email: None,
        },
    )
    .await;
}

#[when("I change my first name to a name that is one character too long")]
async fn i_change_my_first_name_to_a_too_long_name(world: &mut ApiWorld) {
    patch(
        world,
        UserPatch {
            first_name: Some(Pii::new("a".repeat(USER_NAME_MAX_LENGTH as usize + 1))),
            last_name: None,
            email: None,
        },
    )
    .await;
}

#[when("I change nothing in my profile")]
async fn i_change_nothing_in_my_profile(world: &mut ApiWorld) {
    patch(
        world,
        UserPatch {
            first_name: None,
            last_name: None,
            email: None,
        },
    )
    .await;
}

#[when(expr = "I request the change of my email address to {string}")]
async fn i_request_the_change_of_my_email_address(world: &mut ApiWorld, email: String) {
    patch(
        world,
        UserPatch {
            first_name: None,
            last_name: None,
            email: Some(email_of(&email)),
        },
    )
    .await;
}

#[when("I send the profile change:")]
async fn i_send_the_profile_change(world: &mut ApiWorld, step: &Step) {
    let body: serde_json::Value = serde_json::from_str(
        step.docstring
            .as_ref()
            .expect("The profile change must be given as a doc string"),
    )
    .unwrap();
    let client = world.client();
    let url = format!("{}users/me", world.app.get().await.api_address);
    let response = client.patch(url).json(&body).send().await.unwrap();
    world.http.record(Response::read(response).await);
}

#[when(expr = "a visitor who is not signed in changes their first name to {string}")]
async fn a_visitor_changes_their_first_name(world: &mut ApiWorld, first_name: String) {
    let client = world.browser.client().clone();
    let app = world.app.get().await;
    let patch = UserPatch {
        first_name: Some(Pii::new(first_name)),
        last_name: None,
        email: None,
    };
    let response = patch_user_response(&client, app, &patch).await;
    world.http.record(Response::read(response).await);
}

#[when("I open the link of the email change confirmation")]
async fn i_open_the_link_of_the_email_change_confirmation(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let token = get_pending_email_change_token(app, user_id)
        .await
        .expect("A pending email change should have been recorded");
    let url = format!(
        "{}users/{user_id}/email-verification/{token}",
        app.api_address
    );
    let response = reqwest::Client::new().get(url).send().await.unwrap();
    world.http.record(Response::read(response).await);
}

#[then(expr = "the response is my account, named {string} {string}, with the same email address")]
fn the_response_is_my_account_named(world: &mut ApiWorld, first_name: String, last_name: String) {
    let patched: User = world.http.last().json();
    assert_eq!(patched.first_name, Some(Pii::new(first_name)));
    assert_eq!(patched.last_name, Some(Pii::new(last_name)));
    assert_eq!(patched.email, world.user().email);
}

#[then(expr = "my account is named {string} {string}")]
async fn my_account_is_named(world: &mut ApiWorld, first_name: String, last_name: String) {
    let client = world.client();
    let app = world.app.get().await;
    let user = get_current_user(&client, app).await;
    assert_eq!(user.first_name, Some(Pii::new(first_name)));
    assert_eq!(user.last_name, Some(Pii::new(last_name)));
}

#[then("my account still has no first name")]
async fn my_account_still_has_no_first_name(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    assert_eq!(get_current_user(&client, app).await.first_name, None);
}

#[then("the response still shows my current email address")]
fn the_response_still_shows_my_current_email_address(world: &mut ApiWorld) {
    let patched: User = world.http.last().json();
    assert_eq!(patched.email, world.user().email);
}

#[then("that address is still verified")]
fn that_address_is_still_verified(world: &mut ApiWorld) {
    let patched: User = world.http.last().json();
    assert!(patched.email_validated_at.is_some());
}

#[then(regex = r#"^my current email address was told that a change to "([^"]+)" was requested$"#)]
async fn my_current_address_was_told_about_the_change(world: &mut ApiWorld, masked: String) {
    let email = world.my_email();
    let app = world.app.get().await;
    let emails = emails_sent_to(app, &email).await;
    assert!(
        emails.iter().any(|template| matches!(
            template,
            EmailTemplate::EmailChangeRequested { new_email_masked, .. }
                if *new_email_masked == masked
        )),
        "{emails:?}"
    );
}

#[then("no email change is pending")]
async fn no_email_change_is_pending(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    assert!(get_pending_email_change_token(app, user_id).await.is_none());
}

#[then(expr = "my account, as the app now shows it, uses the email {string}")]
async fn my_account_as_now_shown_uses_the_email(world: &mut ApiWorld, email: String) {
    let client = world.client();
    let app = world.app.get().await;
    assert_eq!(
        get_current_user(&client, app).await.email,
        Some(email_of(&email))
    );
}

#[then(expr = "my account, as the app now shows it, uses the verified email {string}")]
async fn my_account_as_now_shown_uses_the_verified_email(world: &mut ApiWorld, email: String) {
    let client = world.client();
    let app = world.app.get().await;
    let user = get_current_user(&client, app).await;
    assert_eq!(user.email, Some(email_of(&email)));
    assert!(user.email_validated_at.is_some());
}

#[then("my account is exactly as it was when I signed up")]
async fn my_account_is_exactly_as_it_was(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let after = get_current_user(&client, app).await;
    let before = world.user();
    assert_eq!(
        after.email, before.email,
        "A refused change must not change the stored email"
    );
    assert_eq!(
        after.email_validated_at, before.email_validated_at,
        "A refused change must not touch email_validated_at"
    );
    assert_eq!(
        after.email_validation_sent_at, before.email_validation_sent_at,
        "A refused change must not re-send the verification email"
    );
}

#[when("I ask how the app is configured")]
async fn i_ask_how_the_app_is_configured(world: &mut ApiWorld) {
    let client = world.browser.client().clone();
    let url = format!("{}front_config", world.app.get().await.api_address);
    let response = client.get(url).send().await.unwrap();
    world.http.record(Response::read(response).await);
}

#[then(regex = r"^the app reports that email is (enabled|disabled)$")]
fn the_app_reports_that_email_is(world: &mut ApiWorld, state: String) {
    let front_config: FrontConfig = world.http.last().json();
    assert_eq!(front_config.email_enabled, state == "enabled");
}

#[then("my email address is verified straight away, without any email")]
async fn my_email_address_is_verified_straight_away(world: &mut ApiWorld) {
    let user = world.user();
    assert!(user.email_validated_at.is_some());
    assert!(user.is_email_validated());
    let user_id = user.id;
    let app = world.app.get().await;
    assert!(
        crate::helpers::user::get_user_email_validation_token(app, user_id)
            .await
            .is_none()
    );
}

#[then(expr = "the response is my account with the verified email {string}")]
fn the_response_is_my_account_with_the_verified_email(world: &mut ApiWorld, email: String) {
    let patched: User = world.http.last().json();
    assert_eq!(patched.email, Some(email_of(&email)));
    assert!(patched.is_email_validated());
}

#[then("the app refuses because local authentication is disabled")]
fn the_app_refuses_because_local_authentication_is_disabled(world: &mut ApiWorld) {
    let response = world.http.last();
    assert_eq!(response.status, StatusCode::FORBIDDEN);
    let body: HashMap<String, String> = response.json();
    assert_eq!(
        body.get("message").unwrap(),
        "Forbidden access: Local authentication is disabled"
    );
}
