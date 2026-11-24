use std::time::Duration;

use cucumber::{given, then, when};
use pretty_assertions::assert_eq;
use reqwest::{Client, StatusCode, header::ORIGIN};
use secrecy::{ExposeSecret, SecretBox};
use serde_json::Value;
use uuid::Uuid;

use universal_inbox::{
    auth::AuthorizeSessionResponse,
    pii::Pii,
    user::{Password, REAUTHENTICATION_REQUIRED_CODE, UserAuthKind, UserPatch},
};

use crate::{
    helpers::{
        TEST_REAUTHENTICATION_WINDOW_SECONDS, TestedApp,
        user::{
            add_local_auth_response, finish_body_with_nonce, front_origin_header,
            patch_user_response, remove_auth_method_response,
            start_add_passkey_registration_response,
        },
    },
    state::ordinal,
    world::{ApiWorld, Response},
};

const PASSWORD: &str = "Very-harD-pasSword-5";

fn unique_email() -> Pii<email_address::EmailAddress> {
    format!("reauth-{}@example.com", Uuid::new_v4())
        .parse()
        .unwrap()
}

pub(super) async fn create_api_token(client: &Client, app: &TestedApp) -> reqwest::Response {
    client
        .post(format!("{}users/me/authentication-tokens", app.api_address))
        .header(ORIGIN, front_origin_header(app))
        .send()
        .await
        .unwrap()
}

async fn run_sensitive_operation(world: &mut ApiWorld, operation: &str) -> reqwest::Response {
    let client = world.client();
    let app = world.app.get().await;
    match operation {
        "change my email address" => {
            let patch = UserPatch {
                email: Some(unique_email()),
                ..Default::default()
            };
            patch_user_response(&client, app, &patch).await
        }
        "add a password" => add_local_auth_response(&client, app, PASSWORD).await,
        "start adding a passkey" => {
            start_add_passkey_registration_response(&client, app, "stale_passkey").await
        }
        "remove my password" => {
            remove_auth_method_response(&client, app, UserAuthKind::Local).await
        }
        "create an API token" => create_api_token(&client, app).await,
        "link single sign-on" => client
            .get(format!("{}auth/link-oidc/authorize", app.api_address))
            .send()
            .await
            .unwrap(),
        _ => panic!("Unknown sensitive operation {operation:?}"),
    }
}

#[given("a sign-in only counts as recent for a short time")]
fn a_sign_in_only_counts_as_recent_for_a_short_time(world: &mut ApiWorld) {
    world
        .app
        .configure()
        .settings
        .application
        .security
        .reauthentication_window_in_seconds = TEST_REAUTHENTICATION_WINDOW_SECONDS;
}

#[when("I wait until my sign-in is no longer recent")]
async fn i_wait_until_my_sign_in_is_no_longer_recent(_world: &mut ApiWorld) {
    tokio::time::sleep(Duration::from_millis(
        u64::from(TEST_REAUTHENTICATION_WINDOW_SECONDS) * 1_000 + 200,
    ))
    .await;
}

#[when(
    regex = r"^I ask to (change my email address|add a password|start adding a passkey|remove my password|create an API token|link single sign-on)$"
)]
async fn i_ask_to_run_a_sensitive_operation(world: &mut ApiWorld, operation: String) {
    let response = run_sensitive_operation(world, &operation).await;
    world.http.record(Response::read(response).await);
}

#[then("the app asks me to sign in again")]
fn the_app_asks_me_to_sign_in_again(world: &mut ApiWorld) {
    let response = world.http.last();
    assert_eq!(response.status, StatusCode::FORBIDDEN);
    let body: Value = response.json();
    assert_eq!(body["code"], REAUTHENTICATION_REQUIRED_CODE);
}

#[when(expr = "I change my first name to {string} and keep my email address")]
async fn i_change_my_first_name(world: &mut ApiWorld, first_name: String) {
    let patch = UserPatch {
        first_name: Some(Pii::new(first_name)),
        email: Some(world.my_email()),
        ..Default::default()
    };
    let client = world.client();
    let app = world.app.get().await;
    let response = patch_user_response(&client, app, &patch).await;
    world.http.record(Response::read(response).await);
}

#[when(expr = "I confirm my identity with the password {string}")]
async fn i_confirm_my_identity_with_a_password(world: &mut ApiWorld, password: String) {
    let client = world.client();
    let app = world.app.get().await;
    let response = client
        .post(format!(
            "{}users/me/reauthentication/password",
            app.api_address
        ))
        .header(ORIGIN, front_origin_header(app))
        .json(&SecretBox::new(Box::new(Password(password))))
        .send()
        .await
        .unwrap();
    world.http.record(Response::read(response).await);
}

#[when(regex = r"^I finish confirming my identity with my (first|second) device$")]
async fn i_finish_confirming_my_identity_with_a_device(world: &mut ApiWorld, which: String) {
    let (challenge, nonce) = world
        .passkeys
        .reauth
        .take()
        .expect("No passkey confirmation was started");
    let client = world.client();
    let app = world.app.get().await;
    let device = &mut world.passkeys.devices[ordinal(&which)];
    let credential = device
        .authenticator
        .do_authentication(app.front_base_url.clone(), challenge)
        .unwrap();
    let response = client
        .post(format!(
            "{}users/me/reauthentication/passkey/finish",
            app.api_address
        ))
        .header(ORIGIN, front_origin_header(app))
        .json(&finish_body_with_nonce(&credential, &nonce))
        .send()
        .await
        .unwrap();
    world.http.record(Response::read(response).await);
}

#[when("I ask to confirm my identity with the identity provider")]
async fn i_ask_to_confirm_my_identity_with_the_identity_provider(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let response = client
        .get(format!(
            "{}auth/reauthenticate-oidc/authorize",
            app.api_address
        ))
        .send()
        .await
        .unwrap();
    world.http.record(Response::read(response).await);
}

#[then("the identity provider's sign-in address asks me to select an account")]
fn the_sign_in_address_asks_me_to_select_an_account(world: &mut ApiWorld) {
    let body: AuthorizeSessionResponse = world.http.last().json();
    assert!(
        body.authorization_url
            .query_pairs()
            .any(|(key, value)| key == "prompt" && value == "select_account"),
        "{}",
        body.authorization_url
    );
}

#[when(
    regex = r"^I use my API token to (create an API token|change my email address|remove my password)$"
)]
async fn i_use_my_api_token_to(world: &mut ApiWorld, operation: String) {
    let jwt = world
        .sessions
        .api_token
        .as_ref()
        .expect("The scenario has no API token")
        .jwt_token
        .expose_secret()
        .0
        .clone();
    let app = world.app.get().await;
    let bearer_client = Client::new();
    let response = match operation.as_str() {
        "create an API token" => bearer_client
            .post(format!("{}users/me/authentication-tokens", app.api_address))
            .header(ORIGIN, front_origin_header(app))
            .bearer_auth(&jwt),
        "change my email address" => bearer_client
            .patch(format!("{}users/me", app.api_address))
            .bearer_auth(&jwt)
            .json(&UserPatch {
                email: Some(unique_email()),
                ..Default::default()
            }),
        "remove my password" => bearer_client
            .delete(format!(
                "{}users/me/auth-methods/{}",
                app.api_address,
                UserAuthKind::Local
            ))
            .header(ORIGIN, front_origin_header(app))
            .bearer_auth(&jwt),
        _ => unreachable!(),
    }
    .send()
    .await
    .unwrap();
    world.http.record(Response::read(response).await);
}
