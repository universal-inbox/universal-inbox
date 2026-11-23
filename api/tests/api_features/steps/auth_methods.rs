use std::collections::HashMap;

use chrono::{TimeDelta, Utc};
use cucumber::{gherkin::Step, given, then, when};
use openidconnect::{
    AccessToken, Audience, EmptyAdditionalClaims, EndUserEmail, IssuerUrl, StandardClaims,
    SubjectIdentifier,
    core::{CoreHmacKey, CoreIdToken, CoreIdTokenClaims, CoreJwsSigningAlgorithm},
};
use pretty_assertions::assert_eq;
use reqwest::StatusCode;
use webauthn_authenticator_rs::{WebauthnAuthenticator, softpasskey::SoftPasskey};
use webauthn_rs::prelude::RequestChallengeResponse;

use universal_inbox::{
    auth::SessionAuthValidationParameters,
    user::{UserAuthKind, UserAuthMethod, UserAuthMethodDisplayInfo},
};
use universal_inbox_api::{
    mailer::EmailTemplate,
    universal_inbox::user::model::{AuthUserId, OpenIdConnectUserAuth, UserAuth},
};

use crate::{
    common::scenarios::SignInMode,
    helpers::{
        TestedApp,
        auth::{
            get_all_user_auths, mock_oidc_introspection, mock_oidc_keys,
            mock_oidc_openid_configuration, mock_oidc_user_info,
        },
        user::{
            add_local_auth_response, emails_sent_to, finish_add_passkey_registration_response,
            finish_body_with_nonce, finish_passkey_registration_response_unauthenticated,
            front_origin_header, link_oidc_pkce_session_response, list_auth_methods_response,
            remove_auth_method_response, split_creation_challenge,
            start_add_passkey_registration_response, start_passkey_authentication_response,
            start_passkey_registration_response, with_discoverable_credential,
        },
    },
    state::{ForeignPasskey, cookie_client},
    steps::passkey::start_and_finish_adding_a_passkey,
    world::{ApiWorld, Response},
};

const PASSWORD: &str = "Very-harD-pasSword-5";

fn kind_of(word: &str) -> UserAuthKind {
    match word {
        "password" => UserAuthKind::Local,
        "passkey" => UserAuthKind::Passkey,
        "single sign-on" => UserAuthKind::OIDCAuthorizationCodePKCE,
        "Google" => UserAuthKind::OIDCGoogleAuthorizationCode,
        _ => panic!("Unknown sign-in method {word:?}"),
    }
}

/// How the app shows a sign-in method that carries no name.
fn plain_display_of(kind: UserAuthKind) -> Option<UserAuthMethodDisplayInfo> {
    match kind {
        UserAuthKind::Local => Some(UserAuthMethodDisplayInfo::Local),
        UserAuthKind::OIDCAuthorizationCodePKCE => {
            Some(UserAuthMethodDisplayInfo::OIDCAuthorizationCodePKCE)
        }
        UserAuthKind::OIDCGoogleAuthorizationCode => {
            Some(UserAuthMethodDisplayInfo::OIDCGoogleAuthorizationCode)
        }
        UserAuthKind::Passkey => None,
    }
}

#[given("the app lets users sign in with single sign-on, a password or a passkey")]
fn the_app_lets_users_sign_in_with_every_method(world: &mut ApiWorld) {
    world.app.configure().sign_in = SignInMode::SingleSignOnPasswordAndPasskey;
}

#[given("the app lets users sign in with a password but not with a passkey")]
fn the_app_lets_users_sign_in_with_a_password_only(world: &mut ApiWorld) {
    world.app.configure().sign_in = SignInMode::Local { passkey: false };
}

#[given("another app lets users sign in with a password or a passkey")]
fn another_app_lets_users_sign_in_with_a_password_or_a_passkey(world: &mut ApiWorld) {
    world.other_app.configure().sign_in = SignInMode::Local { passkey: true };
}

#[when("I list my sign-in methods")]
async fn i_list_my_sign_in_methods(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let response = list_auth_methods_response(&client, app).await;
    world.http.record(Response::read(response).await);
}

#[when("a visitor who is not signed in lists the sign-in methods of the app")]
async fn a_visitor_lists_the_sign_in_methods(world: &mut ApiWorld) {
    let client = world.browser.client().clone();
    let app = world.app.get().await;
    let response = list_auth_methods_response(&client, app).await;
    world.http.record(Response::read(response).await);
}

#[then("I have these sign-in methods:")]
fn i_have_these_sign_in_methods(world: &mut ApiWorld, step: &Step) {
    let methods: Vec<UserAuthMethod> = world.http.last().json();
    let expected: Vec<UserAuthKind> = step
        .table
        .as_ref()
        .expect("The sign-in methods must be given as a table")
        .rows
        .iter()
        .map(|row| kind_of(&row[0]))
        .collect();
    assert_eq!(methods.len(), expected.len());
    for kind in expected {
        let method = methods
            .iter()
            .find(|method| method.kind == kind)
            .unwrap_or_else(|| panic!("No {kind:?} sign-in method in {methods:?}"));
        if let Some(display) = plain_display_of(kind) {
            assert_eq!(method.display_info, display);
        }
    }
}

#[then(regex = r"^the added sign-in method is (a password|a single sign-on|a Google)$")]
fn the_added_sign_in_method_is(world: &mut ApiWorld, method: String) {
    let kind = kind_of(method.trim_start_matches("a "));
    let added: UserAuthMethod = world.http.last().json();
    assert_eq!(added.kind, kind);
    assert_eq!(
        Some(added.display_info),
        plain_display_of(kind),
        "display of the added method"
    );
}

#[then(regex = r#"^the added sign-in method is a passkey named "([^"]+)"$"#)]
fn the_added_sign_in_method_is_a_passkey(world: &mut ApiWorld, username: String) {
    let added: UserAuthMethod = world.http.last().json();
    assert_eq!(added.kind, UserAuthKind::Passkey);
    assert_eq!(
        added.display_info,
        UserAuthMethodDisplayInfo::Passkey { username }
    );
}

#[then(expr = "the database holds {int} sign-in methods for my account")]
async fn the_database_holds_sign_in_methods(world: &mut ApiWorld, expected: usize) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    assert_eq!(get_all_user_auths(app, user_id).await.len(), expected);
}

#[then(regex = r#"^an email tells me that "([^"]+)" was (added|removed) as a sign-in method$"#)]
async fn an_email_tells_me_a_sign_in_method_changed(
    world: &mut ApiWorld,
    method_label: String,
    change: String,
) {
    let email = world.my_email();
    let app = world.app.get().await;
    let emails = emails_sent_to(app, &email).await;
    let found = emails
        .iter()
        .any(|template| match (change.as_str(), template) {
            ("added", EmailTemplate::AuthMethodAdded { method, .. })
            | ("removed", EmailTemplate::AuthMethodRemoved { method, .. }) => {
                *method == method_label
            }
            _ => false,
        });
    assert!(found, "{emails:?}");
}

#[when(regex = r#"^I add the password "([^"]+)" to my account$"#)]
async fn i_add_a_password_to_my_account(world: &mut ApiWorld, password: String) {
    let client = world.client();
    let app = world.app.get().await;
    let response = add_local_auth_response(&client, app, &password).await;
    world.http.record(Response::read(response).await);
}

#[given("I have added a password to my account")]
async fn i_have_added_a_password_to_my_account(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let response = add_local_auth_response(&client, app, PASSWORD).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[then(regex = r#"^the error message mentions "([^"]+)"$"#)]
fn the_error_message_mentions(world: &mut ApiWorld, word: String) {
    let body: HashMap<String, String> = world.http.last().json();
    assert!(
        body.get("message").unwrap().contains(&word),
        "{body:?} does not mention {word:?}"
    );
}

#[when(regex = r#"^I add a passkey named "([^"]+)" to my account$"#)]
async fn i_add_a_passkey_to_my_account(world: &mut ApiWorld, username: String) {
    let client = world.client();
    let app = world.app.get().await;
    let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
    let (response, _credential_id) =
        start_and_finish_adding_a_passkey(&client, app, &mut authenticator, &username).await;
    world.http.record(Response::read(response).await);
}

#[when(regex = r"^I remove my (password|passkey|single sign-on) sign-in method$")]
async fn i_remove_a_sign_in_method(world: &mut ApiWorld, method: String) {
    let client = world.client();
    let app = world.app.get().await;
    let response = remove_auth_method_response(&client, app, kind_of(&method)).await;
    world.http.record(Response::read(response).await);
}

#[given(
    regex = r#"^the identity provider knows the account "([^"]+)" as "([^"]+)" "([^"]+)" with the email "([^"]+)"$"#
)]
async fn the_identity_provider_knows_the_account(
    world: &mut ApiWorld,
    id: String,
    first_name: String,
    last_name: String,
    email: String,
) {
    let app = world.app.get().await;
    mock_oidc_openid_configuration(app).await;
    mock_oidc_keys(app).await;
    mock_oidc_introspection(app, &id, true).await;
    mock_oidc_user_info(app, &id, &first_name, &last_name, &email).await;
}

#[when(regex = r#"^I link single sign-on to my account with an identity token for "([^"]+)"$"#)]
async fn i_link_single_sign_on_to_my_account(world: &mut ApiWorld, email: String) {
    let client = world.client();
    let app = world.app.get().await;
    let issuer = app.oidc_issuer_mock_server.as_ref().unwrap().uri();
    let id_token = CoreIdToken::new(
        CoreIdTokenClaims::new(
            IssuerUrl::new(issuer).unwrap(),
            vec![Audience::new(format!("{email}-client-id-123"))],
            Utc::now() + TimeDelta::try_seconds(120).unwrap(),
            Utc::now(),
            StandardClaims::new(SubjectIdentifier::new("John-Doe".to_string()))
                .set_email(Some(EndUserEmail::new(email))),
            EmptyAdditionalClaims {},
        ),
        &CoreHmacKey::new("secret".as_bytes()),
        CoreJwsSigningAlgorithm::HmacSha256,
        None,
        None,
    )
    .unwrap();
    let response = link_oidc_pkce_session_response(
        &client,
        app,
        &SessionAuthValidationParameters {
            auth_id_token: id_token.to_string().into(),
            access_token: AccessToken::new("fake_token".to_string()),
        },
    )
    .await;
    world.http.record(Response::read(response).await);
}

async fn link_google_account(
    world: &mut ApiWorld,
    google_user_id: &str,
) -> Result<UserAuthMethod, String> {
    let user_id = world.user().id;
    let email = world.account.email();
    let app = world.app.get().await;
    // The full redirect flow needs the identity provider to sign real RSA id tokens: the app's
    // service links the account directly instead.
    let mut transaction = app.repository.begin().await.unwrap();
    let result = app
        .user_service
        .link_oidc_auth_method(
            &mut transaction,
            user_id,
            UserAuth::OIDCGoogleAuthorizationCode(Box::new(OpenIdConnectUserAuth {
                auth_user_id: AuthUserId(google_user_id.to_string()),
                auth_id_token: format!("google-fake-id-token-{google_user_id}").into(),
            })),
            email,
        )
        .await;
    transaction.commit().await.unwrap();
    result.map_err(|err| err.to_string())
}

#[given(regex = r#"^my account is linked to the Google account "([^"]+)"$"#)]
async fn my_account_is_linked_to_a_google_account(world: &mut ApiWorld, google_user_id: String) {
    link_google_account(world, &google_user_id)
        .await
        .expect("Failed to link the Google account");
}

#[when(regex = r#"^I link the Google account "([^"]+)" to my account$"#)]
async fn i_link_a_google_account(world: &mut ApiWorld, google_user_id: String) {
    let result = link_google_account(world, &google_user_id).await;
    world.linked.result = Some(result);
}

#[then("the Google account is linked")]
fn the_google_account_is_linked(world: &mut ApiWorld) {
    let method = world
        .linked
        .result
        .as_ref()
        .expect("No account was linked in this scenario")
        .as_ref()
        .unwrap_or_else(|err| panic!("Linking failed: {err}"));
    assert_eq!(method.kind, UserAuthKind::OIDCGoogleAuthorizationCode);
    assert_eq!(
        method.display_info,
        UserAuthMethodDisplayInfo::OIDCGoogleAuthorizationCode
    );
}

#[then("linking a second Google account is refused")]
fn linking_a_second_google_account_is_refused(world: &mut ApiWorld) {
    let result = world
        .linked
        .result
        .as_ref()
        .expect("No account was linked in this scenario");
    assert!(result.is_err(), "Linking succeeded: {result:?}");
}

const PASSKEY_DISABLED: &str =
    "{\"message\":\"Forbidden access: Passkey authentication is disabled\"}";

#[then("the app refuses because passkeys are disabled")]
fn the_app_refuses_because_passkeys_are_disabled(world: &mut ApiWorld) {
    let response = world.http.last();
    assert_eq!(response.status, StatusCode::FORBIDDEN);
    assert_eq!(response.text(), PASSKEY_DISABLED);
}

#[when(regex = r#"^a visitor starts registering a passkey named "([^"]+)"$"#)]
async fn a_visitor_starts_registering_a_passkey(world: &mut ApiWorld, username: String) {
    let client = world.browser.client().clone();
    let app = world.app.get().await;
    let response = start_passkey_registration_response(&client, app, &username).await;
    world.http.record(Response::read(response).await);
}

#[when(regex = r#"^a visitor starts signing in with the passkey of "([^"]+)"$"#)]
async fn a_visitor_starts_signing_in_with_a_passkey(world: &mut ApiWorld, username: String) {
    let client = world.browser.client().clone();
    let app = world.app.get().await;
    let response = start_passkey_authentication_response(&client, app, &username).await;
    world.http.record(Response::read(response).await);
}

#[when(regex = r#"^I start adding a passkey named "([^"]+)" to my account$"#)]
async fn i_start_adding_a_passkey(world: &mut ApiWorld, username: String) {
    let client = world.client();
    let app = world.app.get().await;
    let response = start_add_passkey_registration_response(&client, app, &username).await;
    world.http.record(Response::read(response).await);
}

#[when("I start confirming my identity with a passkey")]
async fn i_start_confirming_my_identity_with_a_passkey(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let response = client
        .post(format!(
            "{}users/me/reauthentication/passkey/start",
            app.api_address
        ))
        .header(reqwest::header::ORIGIN, front_origin_header(app))
        .send()
        .await
        .unwrap();
    let response = Response::read(response).await;
    if response.status == StatusCode::OK {
        let body = response.text();
        let nonce = serde_json::from_str::<serde_json::Value>(&body).unwrap()["nonce"]
            .as_str()
            .unwrap()
            .to_string();
        let challenge: RequestChallengeResponse = serde_json::from_str(&body).unwrap();
        world.passkeys.reauth = Some((challenge, nonce));
    }
    world.http.record(response);
}

#[given(regex = r#"^a visitor registered a passkey named "([^"]+)" on the other app$"#)]
async fn a_visitor_registered_a_passkey_on_the_other_app(world: &mut ApiWorld, username: String) {
    let app = world.other_app.get().await;
    let origin = app.front_base_url.clone();
    let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
    let client = cookie_client();
    let start = start_passkey_registration_response(&client, app, &username).await;
    assert_eq!(start.status(), StatusCode::OK);
    let (challenge, nonce) = split_creation_challenge(&start.text().await.unwrap());
    let credential = authenticator.do_registration(origin, challenge).unwrap();
    let finish =
        finish_passkey_registration_response_unauthenticated(&client, app, &credential, &nonce)
            .await;
    assert_eq!(finish.status(), StatusCode::OK);
    world.passkeys.foreign = Some(ForeignPasskey {
        authenticator,
        credential,
        nonce,
        username,
    });
}

#[when("that visitor replays the passkey registration on this app")]
async fn the_visitor_replays_the_passkey_registration(world: &mut ApiWorld) {
    let foreign = world.passkeys.foreign.as_ref().expect("No visitor passkey");
    let app = world.app.get().await;
    let client = cookie_client();
    let response = finish_passkey_registration_response_unauthenticated(
        &client,
        app,
        &foreign.credential,
        &foreign.nonce,
    )
    .await;
    world.http.record(Response::read(response).await);
    world.passkeys.signed_in_browser = Some(client);
}

#[when("that visitor replays a passkey sign-in started on the other app on this app")]
async fn the_visitor_replays_a_passkey_sign_in(world: &mut ApiWorld) {
    let other = world.other_app.get().await;
    let origin = other.front_base_url.clone();
    let username = world
        .passkeys
        .foreign
        .as_ref()
        .expect("No visitor passkey")
        .username
        .clone();
    let enabled_login = cookie_client();
    let start = start_passkey_authentication_response(&enabled_login, other, &username).await;
    let body = start.text().await.unwrap();
    let foreign = world.passkeys.foreign.as_mut().expect("No visitor passkey");
    let request_challenge = with_discoverable_credential(
        serde_json::from_str::<RequestChallengeResponse>(&body).unwrap(),
        &foreign.credential.raw_id,
    );
    let login_nonce = serde_json::from_str::<serde_json::Value>(&body).unwrap()["nonce"]
        .as_str()
        .unwrap()
        .to_string();
    let login_credential = foreign
        .authenticator
        .do_authentication(origin, request_challenge)
        .unwrap();
    let app = world.app.get().await;
    let client = cookie_client();
    let response = client
        .post(format!(
            "{}users/passkeys/authentication/finish",
            app.api_address
        ))
        .header(reqwest::header::ORIGIN, front_origin_header(app))
        .json(&finish_body_with_nonce(&login_credential, &login_nonce))
        .send()
        .await
        .unwrap();
    world.http.record(Response::read(response).await);
}

#[when("I replay that passkey registration to add it to my account on this app")]
async fn i_replay_that_passkey_registration_on_my_account(world: &mut ApiWorld) {
    let client = world.client();
    let foreign = world.passkeys.foreign.as_ref().expect("No visitor passkey");
    let app: &TestedApp = world.app.get().await;
    let response =
        finish_add_passkey_registration_response(&client, app, &foreign.credential, &foreign.nonce)
            .await;
    world.http.record(Response::read(response).await);
}
