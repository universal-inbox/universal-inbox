use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use cucumber::{given, then, when};
use pretty_assertions::assert_eq;
use sha2::{Digest, Sha256};

use universal_inbox::integration_connection::IntegrationConnectionId;
use universal_inbox_api::repository::oauth_credential::OAuthCredentialRepository;

use crate::{
    steps::{
        oauth_callback::{Caller, authorization_url, call_back},
        provider::ProviderName,
    },
    world::ApiWorld,
};

fn state_of(url: &url::Url) -> Option<String> {
    url.query_pairs()
        .find(|(key, _)| key == "state")
        .map(|(_, value)| value.to_string())
}

#[given("I started that flow a second time")]
async fn i_started_the_flow_a_second_time(world: &mut ApiWorld) {
    let url = authorization_url(world).await;
    world.oauth_callback.earlier_state = world.oauth_callback.state.take();
    world.oauth_callback.earlier_authorization_url = world.oauth_callback.authorization_url.take();
    world.oauth_callback.state = state_of(&url);
    world.oauth_callback.authorization_url = Some(url);
}

#[when(
    "the provider calls back with the earlier state and a code, for the owner of the connection"
)]
async fn the_provider_calls_back_with_the_earlier_state(world: &mut ApiWorld) {
    let state = world
        .oauth_callback
        .earlier_state
        .clone()
        .expect("The flow was started once only");
    call_back(
        world,
        Caller::Owner,
        format!("state={}&code=late-code", urlencoding::encode(&state)),
    )
    .await;
}

#[when("the provider calls back with that state and no code, for the owner of the connection")]
async fn the_provider_calls_back_without_a_code(world: &mut ApiWorld) {
    let state = world.oauth_callback.state.clone().unwrap();
    call_back(
        world,
        Caller::Owner,
        format!("state={}", urlencoding::encode(&state)),
    )
    .await;
}

async fn credential_of(
    world: &mut ApiWorld,
    id: IntegrationConnectionId,
) -> (Vec<u8>, chrono::DateTime<chrono::Utc>) {
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let credential = app
        .repository
        .get_oauth_credential(&mut transaction, id)
        .await
        .unwrap()
        .expect("The connection has no credential");
    transaction.commit().await.unwrap();
    (credential.encrypted_access_token, credential.updated_at)
}

#[when("I note the credential of my connection")]
async fn i_note_the_credential(world: &mut ApiWorld) {
    let id = world.oauth_callback.connection_id.unwrap();
    world.oauth_callback.noted_credential = Some(credential_of(world, id).await);
}

#[then("the credential of my connection is as I noted it")]
async fn the_credential_is_as_noted(world: &mut ApiWorld) {
    let id = world.oauth_callback.connection_id.unwrap();
    let noted = world
        .oauth_callback
        .noted_credential
        .clone()
        .expect("No credential was noted");
    assert_eq!(credential_of(world, id).await, noted);
}

fn challenge_in(url: &url::Url) -> (String, Option<String>) {
    let pair = |name: &str| {
        url.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.to_string())
    };
    (
        pair("code_challenge").expect("The authorization URL has no PKCE challenge"),
        pair("code_challenge_method"),
    )
}

fn challenge_of(world: &ApiWorld) -> (String, Option<String>) {
    challenge_in(world.oauth_callback.authorization_url.as_ref().unwrap())
}

#[given("the authorization asks for a PKCE challenge, with the method S256")]
#[then("the authorization asks for a PKCE challenge, with the method S256")]
fn the_authorization_asks_for_a_challenge(world: &mut ApiWorld) {
    let (_, method) = challenge_of(world);
    assert_eq!(method.as_deref(), Some("S256"));
}

#[then(expr = "the token request of {provider} carried the verifier of that challenge")]
async fn the_token_request_carried_the_verifier(world: &mut ApiWorld, provider: ProviderName) {
    let (challenge, _) = challenge_of(world);
    let app = world.app.get().await;
    let requests = provider.mock_server(app).received_requests().await.unwrap();
    let token_requests: Vec<_> = requests
        .iter()
        .filter(|request| request.url.path() == "/token")
        .collect();
    assert_eq!(token_requests.len(), 1);
    let pairs: Vec<(String, String)> =
        serde_urlencoded::from_bytes(&token_requests[0].body).unwrap();
    let verifier = pairs
        .iter()
        .find(|(key, _)| key == "code_verifier")
        .map(|(_, value)| value.as_str())
        .expect("The token request has no code_verifier");
    assert_eq!(
        URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
        challenge
    );
}

#[given("the two authorizations have different states and different PKCE challenges")]
fn the_two_authorizations_differ(world: &mut ApiWorld) {
    let earlier_url = world
        .oauth_callback
        .earlier_authorization_url
        .as_ref()
        .expect("The flow was started once only");
    assert_ne!(
        world.oauth_callback.earlier_state,
        world.oauth_callback.state
    );
    assert_ne!(challenge_in(earlier_url).0, challenge_of(world).0);
}

async fn verifier_sent_to_the_token_endpoint(
    world: &mut ApiWorld,
    provider: ProviderName,
) -> String {
    let app = world.app.get().await;
    let requests = provider.mock_server(app).received_requests().await.unwrap();
    let token_requests: Vec<_> = requests
        .iter()
        .filter(|request| request.url.path() == "/token")
        .collect();
    assert_eq!(token_requests.len(), 1);
    let pairs: Vec<(String, String)> =
        serde_urlencoded::from_bytes(&token_requests[0].body).unwrap();
    pairs
        .into_iter()
        .find(|(key, _)| key == "code_verifier")
        .map(|(_, value)| value)
        .expect("The token request has no code_verifier")
}

#[then(
    expr = "the token request of {provider} carried a verifier that RFC 7636 allows, which matches no challenge but that one"
)]
async fn the_verifier_matches_only_its_challenge(world: &mut ApiWorld, provider: ProviderName) {
    let verifier = verifier_sent_to_the_token_endpoint(world, provider).await;
    assert!(
        (43..=128).contains(&verifier.len()),
        "verifier length {}",
        verifier.len()
    );
    assert!(
        verifier
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-._~".contains(c)),
        "the verifier has a reserved character"
    );
    let challenge = |url: &url::Url| challenge_in(url).0;
    let digest = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    assert_eq!(
        digest,
        challenge(world.oauth_callback.authorization_url.as_ref().unwrap())
    );
    assert_ne!(
        digest,
        challenge(
            world
                .oauth_callback
                .earlier_authorization_url
                .as_ref()
                .expect("The flow was started once only")
        )
    );
}
