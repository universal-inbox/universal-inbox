use cucumber::{given, then, when};
use pretty_assertions::{assert_eq, assert_ne};
use reqwest::StatusCode;
use uuid::Uuid;

use universal_inbox::user::{EmailValidationToken, UserId};

use crate::{
    common::settings,
    helpers::user::{get_current_user, get_user_email_validation_token},
    world::{ApiWorld, Response},
};

async fn open_link(world: &mut ApiWorld, link: String) {
    let client = world.browser.client().clone();
    let response = client.get(&link).send().await.unwrap();
    world.verification.link = Some(link);
    world.http.record(Response::read(response).await);
}

async fn my_verification_token(world: &mut ApiWorld) -> EmailValidationToken {
    current_verification_token(world)
        .await
        .expect("The account has no verification token")
}

async fn current_verification_token(world: &mut ApiWorld) -> Option<EmailValidationToken> {
    let user_id = world.user().id;
    let app = world.app.get().await;
    get_user_email_validation_token(app, user_id).await
}

#[when("I ask for the verification email to be sent again")]
async fn i_ask_for_the_verification_email_again(world: &mut ApiWorld) {
    world.verification.previous_token = current_verification_token(world).await;
    let client = world.client();
    let app = world.app.get().await;
    let response = client
        .post(format!("{}users/me/email-verification", app.api_address))
        .send()
        .await
        .unwrap();
    world.http.record(Response::read(response).await);
}

#[then("my verification token is a new one")]
async fn my_verification_token_is_a_new_one(world: &mut ApiWorld) {
    let previous = world
        .verification
        .previous_token
        .take()
        .expect("No verification email was asked for again");
    assert_ne!(previous, my_verification_token(world).await);
}

#[when("I open the link of the verification email")]
async fn i_open_the_link_of_the_verification_email(world: &mut ApiWorld) {
    let token = my_verification_token(world).await;
    let user_id = world.user().id;
    let api_address = world.app.get().await.api_address.clone();
    open_link(
        world,
        format!("{api_address}users/{user_id}/email-verification/{token}"),
    )
    .await;
}

#[when("I open the same link again")]
async fn i_open_the_same_link_again(world: &mut ApiWorld) {
    let link = world
        .verification
        .link
        .clone()
        .expect("No verification link was opened");
    open_link(world, link).await;
}

#[when("I open the link of the verification email with another user's id")]
async fn i_open_the_link_with_another_users_id(world: &mut ApiWorld) {
    let token = my_verification_token(world).await;
    let user_id = UserId(Uuid::new_v4());
    world.verification.link_user_id = Some(user_id);
    let api_address = world.app.get().await.api_address.clone();
    open_link(
        world,
        format!("{api_address}users/{user_id}/email-verification/{token}"),
    )
    .await;
}

#[when("I open a verification link with a token I was never sent")]
async fn i_open_a_link_with_an_unknown_token(world: &mut ApiWorld) {
    let token = EmailValidationToken(Uuid::new_v4());
    let user_id = world.user().id;
    world.verification.link_user_id = Some(user_id);
    let api_address = world.app.get().await.api_address.clone();
    open_link(
        world,
        format!("{api_address}users/{user_id}/email-verification/{token}"),
    )
    .await;
}

#[then("the error message says the verification token is invalid for the user in the link")]
fn the_error_says_the_token_is_invalid_for_the_user(world: &mut ApiWorld) {
    let user_id = world
        .verification
        .link_user_id
        .expect("No verification link was opened");
    let body: std::collections::HashMap<String, String> = world.http.last().json();
    assert_eq!(
        body.get("message").unwrap(),
        &format!("Invalid input data: Invalid email validation token for user {user_id}")
    );
}

#[then("my email address is verified")]
async fn my_email_address_is_verified(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let user = get_current_user(&client, app).await;
    assert!(user.email_validated_at.is_some());
    assert!(user.email_validation_sent_at.is_some());
}

#[then("my email address is still not verified")]
async fn my_email_address_is_still_not_verified(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let user = get_current_user(&client, app).await;
    assert!(user.email_validated_at.is_none());
}

#[then("my verification token is used up")]
async fn my_verification_token_is_used_up(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    assert_eq!(get_user_email_validation_token(app, user_id).await, None);
}

#[given("my verification email was sent just over the validity period ago")]
async fn my_verification_email_was_sent_over_the_validity_period_ago(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let validity_in_hours = settings()
        .application
        .security
        .email_verification_token_validity_in_hours;
    let app = world.app.get().await;
    sqlx::query(
        r#"
            UPDATE "user"
            SET email_validation_sent_at = NOW() - make_interval(hours => $2, mins => 1)
            WHERE id = $1
        "#,
    )
    .bind(user_id.0)
    .bind(validity_in_hours as i32)
    .execute(&*app.repository.pool)
    .await
    .unwrap();
}

#[then("the app refuses the link")]
fn the_app_refuses_the_link(world: &mut ApiWorld) {
    assert_eq!(world.http.last().status, StatusCode::BAD_REQUEST);
}
