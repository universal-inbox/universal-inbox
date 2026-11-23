use cucumber::{given, then, when};
use pretty_assertions::assert_eq;
use regex::Regex;
use reqwest::{Client, StatusCode, header::ORIGIN};
use serde_json::Value;
use webauthn_authenticator_rs::{WebauthnAuthenticator, softpasskey::SoftPasskey};
use webauthn_rs::prelude::{CreationChallengeResponse, RequestChallengeResponse};

use crate::{
    helpers::user::{
        finish_add_passkey_registration_response, finish_body_with_nonce,
        finish_passkey_authentication_response,
        finish_passkey_registration_response_unauthenticated, front_origin_header,
        split_creation_challenge, start_add_passkey_registration_response,
        start_passkey_authentication_response, start_passkey_registration_response,
        with_discoverable_credential,
    },
    state::{Device, PendingSignIn, Registration, cookie_client, ordinal},
    steps::passkey::split_request_challenge,
    world::{ApiWorld, Response},
};

const BOGUS_NONCE: &str = "AAAAAAAAAAAAAAAAAAAAAA";

fn begin_registration(world: &mut ApiWorld, browser: Client, response: Response) {
    if response.status == StatusCode::OK {
        let (challenge, nonce) = split_creation_challenge(&response.text());
        world.passkeys.registration = Some(Registration {
            browser,
            authenticator: WebauthnAuthenticator::new(SoftPasskey::new(true)),
            challenge,
            nonce,
            credential: None,
        });
    }
    world.http.record(response);
}

#[given(expr = "a visitor registered a passkey for the username {string}")]
async fn a_visitor_registered_a_passkey(world: &mut ApiWorld, username: String) {
    let browser = cookie_client();
    let app = world.app.get().await;
    let response = start_passkey_registration_response(&browser, app, &username).await;
    assert_eq!(response.status(), StatusCode::OK);
    let (challenge, nonce) = split_creation_challenge(&response.text().await.unwrap());
    let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
    let credential = authenticator
        .do_registration(app.front_base_url.clone(), challenge)
        .expect("The software authenticator could not register the passkey");
    let response =
        finish_passkey_registration_response_unauthenticated(&browser, app, &credential, &nonce)
            .await;
    assert_eq!(response.status(), StatusCode::OK);
    world.passkeys.registered_credential_id = Some(credential.id.clone());
    world.passkeys.devices.push(Device {
        authenticator,
        username,
        credential_id: serde_json::to_value(&credential.raw_id).unwrap(),
    });
}

#[when(expr = "a visitor starts registering a passkey for the username {string}")]
async fn a_visitor_starts_registering_a_passkey(world: &mut ApiWorld, username: String) {
    let browser = cookie_client();
    let app = world.app.get().await;
    let response = start_passkey_registration_response(&browser, app, &username).await;
    begin_registration(world, browser, Response::read(response).await);
}

#[when(expr = "I start adding a passkey named {string}")]
async fn i_start_adding_a_passkey(world: &mut ApiWorld, username: String) {
    let browser = world.client();
    let app = world.app.get().await;
    let response = start_add_passkey_registration_response(&browser, app, &username).await;
    begin_registration(world, browser, Response::read(response).await);
}

#[then("the app issued a nonce for the registration")]
fn the_app_issued_a_nonce(world: &mut ApiWorld) {
    let registration = world.passkeys.registration.as_ref().unwrap();
    assert!(
        !registration.nonce.is_empty(),
        "start response must include a non-empty nonce"
    );
}

#[when("the device answers the registration challenge")]
async fn the_device_answers_the_registration_challenge(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let registration = world
        .passkeys
        .registration
        .as_mut()
        .expect("No passkey registration was started");
    registration.credential = Some(
        registration
            .authenticator
            .do_registration(app.front_base_url.clone(), registration.challenge.clone())
            .expect("The software authenticator could not register the passkey"),
    );
}

async fn finish_registration(world: &mut ApiWorld, nonce: Option<&str>, origin: Option<&str>) {
    let app = world.app.get().await;
    let registration = world
        .passkeys
        .registration
        .as_ref()
        .expect("No passkey registration was started");
    let credential = registration
        .credential
        .as_ref()
        .expect("The device did not answer the registration challenge");
    let nonce = nonce.unwrap_or(&registration.nonce);
    let response = match origin {
        None => {
            finish_passkey_registration_response_unauthenticated(
                &registration.browser,
                app,
                credential,
                nonce,
            )
            .await
        }
        Some(origin) => registration
            .browser
            .post(format!(
                "{}users/passkeys/registration/finish",
                app.api_address
            ))
            .header(ORIGIN, origin)
            .json(&finish_body_with_nonce(credential, nonce))
            .send()
            .await
            .unwrap(),
    };
    world.http.record(Response::read(response).await);
}

#[when("the registration is finished through the endpoint for new accounts")]
async fn the_registration_is_finished(world: &mut ApiWorld) {
    let origin = front_origin_header(world.app.get().await);
    finish_registration(world, None, Some(&origin)).await;
}

#[when("the registration is finished with a nonce the app did not issue")]
async fn the_registration_is_finished_with_a_bogus_nonce(world: &mut ApiWorld) {
    finish_registration(world, Some(BOGUS_NONCE), None).await;
}

#[when(expr = "the registration is finished from the website {string}")]
async fn the_registration_is_finished_from_a_website(world: &mut ApiWorld, origin: String) {
    finish_registration(world, None, Some(&origin)).await;
}

#[when("the registration is finished through the endpoint for adding a passkey")]
async fn the_registration_is_finished_for_adding_a_passkey(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let registration = world.passkeys.registration.as_ref().unwrap();
    let credential = registration.credential.as_ref().unwrap();
    let response = finish_add_passkey_registration_response(
        &registration.browser,
        app,
        credential,
        &registration.nonce,
    )
    .await;
    world.http.record(Response::read(response).await);
}

#[then("the response mentions neither the database, the username nor any identifier")]
fn the_response_leaks_nothing(world: &mut ApiWorld) {
    let body = world.http.last().text();
    let username = world
        .passkeys
        .devices
        .last()
        .map(|device| device.username.clone())
        .expect("No passkey was registered");
    for needle in [
        "user_auth_username_key",
        "duplicate key",
        "Database error",
        "DatabaseError",
        "unique constraint",
        "AlreadyExists",
        "sqlx",
        username.as_str(),
    ] {
        assert!(
            !body.contains(needle),
            "the response leaks {needle:?}: {body}"
        );
    }
    let uuid =
        Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
            .unwrap();
    assert!(!uuid.is_match(&body), "the response leaks a UUID: {body}");
}

#[then("the app does not answer with a success")]
fn the_app_does_not_answer_with_a_success(world: &mut ApiWorld) {
    let status = world.http.last().status;
    assert!(!status.is_success(), "unexpected success {status}");
}

fn creation_challenge(world: &ApiWorld, index: usize) -> CreationChallengeResponse {
    serde_json::from_str(&world.http.all()[index].text())
        .expect("The answer must be a passkey registration challenge")
}

fn request_challenge(world: &ApiWorld, index: usize) -> RequestChallengeResponse {
    serde_json::from_str(&world.http.all()[index].text())
        .expect("The answer must be a passkey sign-in challenge")
}

#[then("both answers are registration challenges for the same relying party")]
fn both_answers_are_registration_challenges(world: &mut ApiWorld) {
    assert_eq!(
        creation_challenge(world, 0).public_key.rp.id,
        creation_challenge(world, 1).public_key.rp.id
    );
}

#[then(
    expr = "the first challenge is for the user {string} and the second one for the user {string}"
)]
fn the_challenges_are_for_the_users(world: &mut ApiWorld, first: String, second: String) {
    assert_eq!(creation_challenge(world, 0).public_key.user.name, first);
    assert_eq!(creation_challenge(world, 1).public_key.user.name, second);
}

#[then("the second answer does not say that the username already exists")]
fn the_second_answer_does_not_say_the_username_exists(world: &mut ApiWorld) {
    let body = world.http.all()[1].text();
    assert!(!body.contains("already exists"), "{body}");
    assert!(!body.contains("AlreadyExists"), "{body}");
}

#[when(expr = "a(nother) visitor starts a passkey sign-in as {string}")]
async fn a_visitor_starts_a_passkey_sign_in(world: &mut ApiWorld, username: String) {
    let browser = cookie_client();
    let app = world.app.get().await;
    let response = start_passkey_authentication_response(&browser, app, &username).await;
    let response = Response::read(response).await;
    if response.status == StatusCode::OK {
        let (challenge, nonce) = split_request_challenge(&response.text());
        world.passkeys.started.push(PendingSignIn {
            browser,
            challenge,
            nonce,
        });
    }
    world.http.record(response);
}

#[then("both answers are sign-in challenges for the same relying party")]
fn both_answers_are_sign_in_challenges(world: &mut ApiWorld) {
    assert_eq!(
        request_challenge(world, 0).public_key.rp_id,
        request_challenge(world, 1).public_key.rp_id
    );
}

#[then("neither sign-in challenge lists a credential the sign-in may use")]
fn neither_challenge_lists_credentials(world: &mut ApiWorld) {
    for index in 0..2 {
        assert!(
            request_challenge(world, index)
                .public_key
                .allow_credentials
                .is_empty(),
            "the answer exposes allow_credentials: {}",
            world.http.all()[index].text()
        );
    }
}

#[then("the first answer does not contain the ID of the registered credential")]
fn the_first_answer_does_not_leak_the_credential_id(world: &mut ApiWorld) {
    let credential_id = world
        .passkeys
        .registered_credential_id
        .clone()
        .expect("No passkey was registered");
    let body = world.http.all()[0].text();
    assert!(!body.contains(&credential_id), "{body}");
}

#[then(
    expr = "the second answer does not repeat the username {string} nor say that an item was not found"
)]
fn the_second_answer_does_not_leak_the_username(world: &mut ApiWorld, username: String) {
    let body = world.http.all()[1].text();
    assert!(!body.contains(&username), "{body}");
    assert!(!body.contains("Item not found"), "{body}");
    assert!(!body.contains("ItemNotFound"), "{body}");
}

fn without_ceremony_values(body: &str) -> Value {
    let mut value: Value = serde_json::from_str(body).unwrap();
    value["nonce"] = "<redacted>".into();
    value["publicKey"]["challenge"] = "<redacted>".into();
    value
}

#[then("both answers have the same structure once their challenge and nonce are left out")]
fn both_answers_have_the_same_structure(world: &mut ApiWorld) {
    assert_eq!(
        without_ceremony_values(&world.http.all()[0].text()),
        without_ceremony_values(&world.http.all()[1].text()),
        "known and unknown start responses must be structurally identical"
    );
}

#[when(
    regex = r#"^the device of "([^"]+)" completes the (first|second) sign-in that was started$"#
)]
async fn the_device_completes_a_started_sign_in(
    world: &mut ApiWorld,
    username: String,
    which: String,
) {
    let app = world.app.get().await;
    let pending = world.passkeys.started.remove(ordinal(&which));
    assert!(pending.challenge.public_key.allow_credentials.is_empty());
    let device = world
        .passkeys
        .devices
        .iter_mut()
        .find(|device| device.username == username)
        .expect("No device holds a passkey for that username");
    let challenge = with_discoverable_credential(pending.challenge, &device.credential_id);
    let credential = device
        .authenticator
        .do_authentication(app.front_base_url.clone(), challenge)
        .unwrap();
    let response =
        finish_passkey_authentication_response(&pending.browser, app, &credential, &pending.nonce)
            .await;
    world.http.record(Response::read(response).await);
}
