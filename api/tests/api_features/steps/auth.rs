use std::time::SystemTime;

use cucumber::{given, then, when};
use email_address::EmailAddress;
use openidconnect::AccessToken;
use pretty_assertions::{assert_eq, assert_ne};
use reqwest::{Client, StatusCode, header::COOKIE};
use uuid::Uuid;

use universal_inbox::{
    auth::{AuthorizeSessionResponse, CloseSessionResponse, SessionAuthValidationParameters},
    pii::Pii,
    user::{User, UserAuthKind},
};

use universal_inbox_api::universal_inbox::user::model::UserAuth;

use crate::{
    common::scenarios::LocalAuth,
    helpers::{
        auth::{
            fetch_auth_tokens_for_user, get_user_auth, mock_oidc_introspection, mock_oidc_keys,
            mock_oidc_openid_configuration, mock_oidc_user_info,
        },
        user::{
            create_user, get_current_user_response, get_current_user_with_bearer_response,
            login_and_copy_session, logout_user_response, register_user,
        },
    },
    steps::id_tokens::{FIRST_ID_TOKEN, SECOND_ID_TOKEN},
    world::{ApiWorld, Response, Session},
};

const PASSWORD: &str = "Very-harD-pasSword-5";

/// The address of each account is unique: the login throttle is kept in Redis, which every app
/// of every test shares.
fn unique_email() -> Pii<EmailAddress> {
    format!("user-{}@example.com", Uuid::new_v4())
        .parse()
        .unwrap()
}

fn id_token(which: &str) -> &'static str {
    match which {
        "first" => FIRST_ID_TOKEN,
        "second" => SECOND_ID_TOKEN,
        _ => panic!("Unknown identity token {which:?}"),
    }
}

#[given("the app lets users sign in with a password or a passkey")]
fn the_app_lets_users_sign_in_with_a_password_or_a_passkey(world: &mut ApiWorld) {
    let config = world.app.configure();
    config.identity_provider = false;
    config.local_auth = Some(LocalAuth { passkey: true });
}

#[given("the identity provider is available")]
async fn the_identity_provider_is_available(world: &mut ApiWorld) {
    let app = world.app.get().await;
    mock_oidc_openid_configuration(app).await;
    mock_oidc_keys(app).await;
}

#[given(
    regex = r#"^the identity provider knows me as "([^"]+)" "([^"]+)" with the email "([^"]+)"$"#
)]
async fn the_identity_provider_knows_me(
    world: &mut ApiWorld,
    first_name: String,
    last_name: String,
    email: String,
) {
    let app = world.app.get().await;
    mock_oidc_openid_configuration(app).await;
    mock_oidc_keys(app).await;
    mock_oidc_introspection(app, "1234", true).await;
    mock_oidc_user_info(app, "1234", &first_name, &last_name, &email).await;
}

#[given("the identity provider rejects my access token")]
async fn the_identity_provider_rejects_my_access_token(world: &mut ApiWorld) {
    let app = world.app.get().await;
    mock_oidc_openid_configuration(app).await;
    mock_oidc_keys(app).await;
    mock_oidc_introspection(app, "1234", false).await;
}

#[given("I have an account with a password")]
async fn i_have_an_account_with_a_password(world: &mut ApiWorld) {
    let email = unique_email();
    let app = world.app.get().await;
    let user = create_user(app, email.clone(), PASSWORD).await;
    world.account.set(email, PASSWORD, user);
}

pub(super) async fn sign_up(world: &mut ApiWorld, email: Pii<EmailAddress>) {
    let app = world.app.get().await;
    let (client, user) = register_user(app, email.clone(), PASSWORD).await;
    world.account.set(email, PASSWORD, user.clone());
    world.session = Some(Session { client, user });
}

#[given("I have signed up with a password")]
async fn i_have_signed_up_with_a_password(world: &mut ApiWorld) {
    sign_up(world, unique_email()).await;
}

#[given(regex = r#"^I have signed up with the email "([^"]+)" and a password$"#)]
async fn i_have_signed_up_with_an_email_and_a_password(world: &mut ApiWorld, email: String) {
    sign_up(world, email.parse().unwrap()).await;
}

async fn sign_in_with_my_password(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let copied = login_and_copy_session(app, world.account.email(), world.account.password()).await;
    if world.session.is_none() {
        world.session = Some(Session {
            client: copied.client.clone(),
            user: world.account.user().clone(),
        });
    }
    world.sessions.open(copied);
}

#[given("I signed in with my password")]
async fn i_signed_in_with_my_password(world: &mut ApiWorld) {
    sign_in_with_my_password(world).await;
}

#[given("I signed in again with my password, in another session")]
async fn i_signed_in_again_with_my_password(world: &mut ApiWorld) {
    sign_in_with_my_password(world).await;
}

#[when(regex = r"^I sign in with the (first|second) identity token$")]
async fn i_sign_in_with_an_identity_token(world: &mut ApiWorld, which: String) {
    let url = format!("{}auth/session", world.app.get().await.api_address);
    let response = world
        .browser
        .client()
        .post(url)
        .json(&SessionAuthValidationParameters {
            auth_id_token: id_token(&which).to_string().into(),
            access_token: AccessToken::new("fake_access_token".to_string()),
        })
        .send()
        .await
        .unwrap();
    world.http.record(Response::read(response).await);
}

async fn current_user_of_the_browser(world: &mut ApiWorld) -> User {
    let url = format!("{}users/me", world.app.get().await.api_address);
    world
        .browser
        .client()
        .get(url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[then(regex = r#"^my account has the first name "([^"]+)" and the last name "([^"]+)"$"#)]
async fn my_account_has_the_names(world: &mut ApiWorld, first_name: String, last_name: String) {
    let user = current_user_of_the_browser(world).await;
    assert_eq!(user.first_name, Some(Pii::new(first_name)));
    assert_eq!(user.last_name, Some(Pii::new(last_name)));
}

#[then(regex = r"^my account stores the (first|second) identity token$")]
async fn my_account_stores_the_identity_token(world: &mut ApiWorld, which: String) {
    let user = current_user_of_the_browser(world).await;
    let app = world.app.get().await;
    let user_auth = get_user_auth(app, user.id, UserAuthKind::OIDCAuthorizationCodePKCE).await;
    let UserAuth::OIDCAuthorizationCodePKCE(user_auth) = &user_auth else {
        panic!("User auth is not OIDCAuthorizationCodePKCE");
    };
    assert_eq!(user_auth.auth_id_token, id_token(&which).to_string().into());
}

#[then("no API access token exists for my account")]
async fn no_api_access_token_exists_for_my_account(world: &mut ApiWorld) {
    let user = current_user_of_the_browser(world).await;
    let app = world.app.get().await;
    assert_eq!(fetch_auth_tokens_for_user(app, user.id).await.len(), 0);
}

#[when("I ask to sign in with the identity provider")]
async fn i_ask_to_sign_in_with_the_identity_provider(world: &mut ApiWorld) {
    let url = format!(
        "{}auth/session/authorize",
        world.app.get().await.api_address
    );
    let response = world.browser.client().get(url).send().await.unwrap();
    let response = Response::read(response).await;
    let body: AuthorizeSessionResponse = response.json();
    world.authorizations.urls.push(body.authorization_url);
    world.http.record(response);
}

#[then(expr = "the app answered {int} every time")]
fn the_app_answered_every_time(world: &mut ApiWorld, status: u16) {
    for response in world.http.all() {
        assert_eq!(response.status, status);
    }
}

#[then("the second address differs from the first")]
fn the_second_address_differs_from_the_first(world: &mut ApiWorld) {
    let urls = &world.authorizations.urls;
    assert_ne!(
        urls[0], urls[1],
        "authorize_session returned the cached authorization URL; the cached entry was not \
         cleared and a stale CSRF state would be reused by the OIDC callback \
         (universal-inbox-bkj.15)"
    );
}

#[then(regex = r#"^the two addresses carry a different "([^"]+)"$"#)]
fn the_two_addresses_carry_a_different_parameter(world: &mut ApiWorld, parameter: String) {
    let value = |index: usize| {
        world.authorizations.urls[index]
            .query_pairs()
            .find(|(key, _)| *key == parameter)
            .map(|(_, value)| value.into_owned())
    };
    let (first, second) = (value(0), value(1));
    assert!(
        first.is_some(),
        "first authorization URL has no `{parameter}`"
    );
    assert!(
        second.is_some(),
        "second authorization URL has no `{parameter}`"
    );
    assert_ne!(
        first, second,
        "authorize_session returned an authorization URL with the same CSRF state across two \
         calls: the cached URL was not cleared"
    );
}

#[when("I sign out")]
async fn i_sign_out(world: &mut ApiWorld) {
    let api_address = world.app.get().await.api_address.clone();
    let client = world.client();
    let response = logout_user_response(&client, &api_address).await;
    world.http.record(Response::read(response).await);
}

#[when(regex = r"^I sign out of my (first|second) session$")]
async fn i_sign_out_of_a_session(world: &mut ApiWorld, which: String) {
    let api_address = world.app.get().await.api_address.clone();
    let client = world.sessions.nth(&which).client.clone();
    let response = logout_user_response(&client, &api_address).await;
    world.http.record(Response::read(response).await);
}

#[then("the session cookie is cleared")]
async fn the_session_cookie_is_cleared(world: &mut ApiWorld) {
    let cookie_name = world.app.get().await.session_cookie_name();
    let cookies = &world.http.last().cookies;
    assert!(!cookies.is_empty(), "The response sets no cookie");
    for cookie in cookies {
        assert_eq!(cookie.name, cookie_name);
        assert_eq!(cookie.value, "");
        assert!(cookie.expires.unwrap() < SystemTime::now());
    }
}

#[then("I am sent to the identity provider's logout page for my session")]
async fn i_am_sent_to_the_identity_provider_logout_page(world: &mut ApiWorld) {
    let close_session_response: CloseSessionResponse = world.http.last().json();
    let user_id = world.user().id;
    let app = world.app.get().await;
    let oidc_issuer_mock_server_url = app.oidc_issuer_mock_server.as_ref().unwrap().uri();
    let user_auth = get_user_auth(app, user_id, UserAuthKind::OIDCAuthorizationCodePKCE).await;
    let UserAuth::OIDCAuthorizationCodePKCE(user_auth) = user_auth else {
        panic!("User auth is not OIDCAuthorizationCodePKCE");
    };
    assert_eq!(
        close_session_response.logout_url.to_string(),
        format!(
            "{oidc_issuer_mock_server_url}/end_session?{}",
            serde_urlencoded::to_string([
                (
                    "id_token_hint",
                    user_auth.auth_id_token.expose_secret().to_string()
                ),
                ("post_logout_redirect_uri", app.front_base_url.to_string())
            ])
            .unwrap()
        )
    );
}

#[then(regex = r"^a copy of my session cookie is (accepted|rejected)$")]
async fn a_copy_of_my_session_cookie_is_accepted_or_rejected(
    world: &mut ApiWorld,
    outcome: String,
) {
    let cookie = world.sessions.latest().cookie.clone();
    let url = format!("{}users/me", world.app.get().await.api_address);
    let response = Client::new()
        .get(url)
        .header(COOKIE, cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), expected_status(&outcome));
}

#[then("my session token is rejected as a bearer token")]
async fn my_session_token_is_rejected_as_a_bearer_token(world: &mut ApiWorld) {
    let jwt = world.sessions.latest().jwt();
    let app = world.app.get().await;
    let response = get_current_user_with_bearer_response(app, &jwt).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[then(regex = r"^my (first|second) session is (still signed in|signed out)$")]
async fn my_session_is_signed_in_or_out(world: &mut ApiWorld, which: String, state: String) {
    let client = world.sessions.nth(&which).client.clone();
    let app = world.app.get().await;
    let response = get_current_user_response(&client, app).await;
    let expected = if state == "signed out" {
        StatusCode::UNAUTHORIZED
    } else {
        StatusCode::OK
    };
    assert_eq!(response.status(), expected);
}

fn expected_status(outcome: &str) -> StatusCode {
    match outcome {
        "accepted" => StatusCode::OK,
        "rejected" => StatusCode::UNAUTHORIZED,
        _ => unreachable!(),
    }
}
