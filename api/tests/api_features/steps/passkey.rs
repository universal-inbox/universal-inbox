use cucumber::{given, then, when};
use pretty_assertions::assert_eq;
use reqwest::{Client, StatusCode};
use serde_json::Value;
use uuid::Uuid;
use webauthn_authenticator_rs::{WebauthnAuthenticator, softpasskey::SoftPasskey};
use webauthn_rs::prelude::RequestChallengeResponse;

use universal_inbox::user::{User, UserAuthKind};

use crate::{
    helpers::{
        TestedApp,
        user::{
            finish_add_passkey_registration_response, finish_passkey_authentication_response,
            get_current_user_response, remove_auth_method_response, split_creation_challenge,
            start_add_passkey_registration_response, start_passkey_authentication_response,
            with_discoverable_credential,
        },
    },
    state::{Device, PendingSignIn, cookie_client, ordinal},
    world::{ApiWorld, Response},
};

pub(super) fn split_request_challenge(body: &str) -> (RequestChallengeResponse, String) {
    let value: Value = serde_json::from_str(body).unwrap();
    let nonce = value["nonce"].as_str().unwrap().to_string();
    (serde_json::from_str(body).unwrap(), nonce)
}

/// Start adding a passkey named `username` to the signed-in `client`'s account, answer the
/// challenge with `authenticator` and finish. Returns the answer to the finish request and the
/// credential ID.
pub(super) async fn start_and_finish_adding_a_passkey(
    client: &Client,
    app: &TestedApp,
    authenticator: &mut WebauthnAuthenticator<SoftPasskey>,
    username: &str,
) -> (reqwest::Response, Value) {
    let response = start_add_passkey_registration_response(client, app, username).await;
    assert_eq!(response.status(), StatusCode::OK);
    let (creation_challenge, nonce) = split_creation_challenge(&response.text().await.unwrap());
    let credential = authenticator
        .do_registration(app.front_base_url.clone(), creation_challenge)
        .unwrap();
    let response = finish_add_passkey_registration_response(client, app, &credential, &nonce).await;
    (response, serde_json::to_value(&credential.raw_id).unwrap())
}

/// Register a passkey for the signed-in `client` and return its username and credential ID.
async fn add_passkey(
    client: &Client,
    app: &TestedApp,
    authenticator: &mut WebauthnAuthenticator<SoftPasskey>,
) -> (String, Value) {
    let username = format!("passkey_{}", Uuid::new_v4().simple());
    let (response, credential_id) =
        start_and_finish_adding_a_passkey(client, app, authenticator, &username).await;
    assert_eq!(response.status(), StatusCode::OK);
    (username, credential_id)
}

async fn register_a_passkey_on_a_new_device(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
    let (username, credential_id) = add_passkey(&client, app, &mut authenticator).await;
    world.passkeys.devices.push(Device {
        authenticator,
        username,
        credential_id,
    });
}

#[given("I have registered a passkey")]
async fn i_have_registered_a_passkey(world: &mut ApiWorld) {
    register_a_passkey_on_a_new_device(world).await;
}

#[when("I register another passkey on a new device")]
async fn i_register_another_passkey_on_a_new_device(world: &mut ApiWorld) {
    register_a_passkey_on_a_new_device(world).await;
}

#[when("I start a passkey sign-in from a signed-out browser")]
async fn i_start_a_passkey_sign_in(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let device = &world.passkeys.devices[0];
    let browser = cookie_client();
    let response = start_passkey_authentication_response(&browser, app, &device.username).await;
    assert_eq!(response.status(), StatusCode::OK);
    let (challenge, nonce) = split_request_challenge(&response.text().await.unwrap());
    world.passkeys.pending = Some(PendingSignIn {
        browser,
        challenge: with_discoverable_credential(challenge, &device.credential_id),
        nonce,
    });
}

#[when("I remove my passkey")]
async fn i_remove_my_passkey(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let response = remove_auth_method_response(&client, app, UserAuthKind::Passkey).await;
    world.http.record(Response::read(response).await);
}

#[when(regex = r"^I complete the passkey sign-in with my (first|second) device$")]
async fn i_complete_the_passkey_sign_in(world: &mut ApiWorld, which: String) {
    let pending = world
        .passkeys
        .pending
        .take()
        .expect("No passkey sign-in was started");
    let app = world.app.get().await;
    let device = &mut world.passkeys.devices[ordinal(&which)];
    let credential = device
        .authenticator
        .do_authentication(app.front_base_url.clone(), pending.challenge)
        .unwrap();
    let response =
        finish_passkey_authentication_response(&pending.browser, app, &credential, &pending.nonce)
            .await;
    world.http.record(Response::read(response).await);
    world.passkeys.signed_in_browser = Some(pending.browser);
}

#[then("the app answers a client error")]
fn the_app_answers_a_client_error(world: &mut ApiWorld) {
    let status = world.http.last().status;
    assert!(status.is_client_error(), "unexpected status {status}");
}

#[then("that browser is signed in as me")]
async fn that_browser_is_signed_in_as_me(world: &mut ApiWorld) {
    let browser = world
        .passkeys
        .signed_in_browser
        .clone()
        .expect("No browser finished a passkey sign-in");
    let app = world.app.get().await;
    let response = get_current_user_response(&browser, app).await;
    assert_eq!(response.status(), StatusCode::OK);
    let current_user: User = response.json().await.unwrap();
    assert_eq!(current_user.id, world.account.user().id);
}

#[then("that browser is not signed in")]
async fn that_browser_is_not_signed_in(world: &mut ApiWorld) {
    let browser = world
        .passkeys
        .signed_in_browser
        .clone()
        .expect("No browser finished a passkey sign-in");
    let app = world.app.get().await;
    let response = get_current_user_response(&browser, app).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
