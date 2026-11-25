use cucumber::{given, then, when};
use pretty_assertions::assert_eq;
use reqwest::StatusCode;
use serde_json::json;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path},
};

use universal_inbox::integration_connection::{
    IntegrationConnectionId, IntegrationConnectionStatus, provider::IntegrationProviderKind,
};
use universal_inbox_api::{
    repository::oauth_credential::OAuthCredentialRepository,
    universal_inbox::integration_connection::service::OAUTH_STATE_PREFIX,
};

use crate::{
    steps::{integration_connections::stored_connection, provider::ProviderName},
    world::ApiWorld,
};

use super::oauth_callback::{
    Caller, GOOGLE_ACCESS_TOKEN, authorization_url_of, call_back, front, new_connection,
    state_query,
};

#[given(expr = "I have a new {provider} connection, not authorized yet")]
async fn i_have_a_new_connection(world: &mut ApiWorld, provider: ProviderName) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let connection = new_connection(app, user_id, provider.0, None, None).await;
    world.oauth_callback.connection_id = Some(connection.id);
}

#[when("the other user asks for the authorization URL of my connection")]
async fn the_other_user_asks_for_my_authorization_url(world: &mut ApiWorld) {
    let id = world.oauth_callback.connection_id.unwrap();
    let client = world.other_session.as_ref().unwrap().client.clone();
    let app = world.app.get().await;
    let response = client
        .get(format!("{}oauth/authorize-url/{id}", app.api_address))
        .send()
        .await
        .expect("Failed to start OAuth authorization");
    world.oauth_callback.status = Some(response.status());
}

#[then("that request is refused as forbidden")]
fn that_request_is_forbidden(world: &mut ApiWorld) {
    assert_eq!(world.oauth_callback.status, Some(StatusCode::FORBIDDEN));
}

#[given("the other user has a new Google Mail connection, not authorized yet")]
async fn the_other_user_has_a_connection(world: &mut ApiWorld) {
    let user_id = world.other_session.as_ref().unwrap().user.id;
    let app = world.app.get().await;
    let connection = new_connection(
        app,
        user_id,
        IntegrationProviderKind::GoogleMail,
        None,
        None,
    )
    .await;
    world.oauth_callback.other_connection_id = Some(connection.id);
}

#[given("a state of mine naming that connection of the other user is stored")]
async fn a_state_naming_the_other_connection_is_stored(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let other_id = world.oauth_callback.other_connection_id.unwrap();
    let app = world.app.get().await;
    let own = new_connection(
        app,
        user_id,
        IntegrationProviderKind::GoogleMail,
        None,
        None,
    )
    .await;
    let issued_state = authorization_url_of(world, own.id)
        .await
        .query_pairs()
        .find(|(key, _)| key == "state")
        .map(|(_, value)| value.to_string())
        .unwrap();
    let app = world.app.get().await;
    let mut conn = app.cache.connection_manager.clone();
    let stored: String =
        redis::AsyncCommands::get(&mut conn, format!("{OAUTH_STATE_PREFIX}{issued_state}"))
            .await
            .expect("Failed to read the issued OAuth state");
    let mut data: serde_json::Value = serde_json::from_str(&stored).unwrap();
    data["integration_connection_id"] = json!(other_id);
    let state = uuid::Uuid::new_v4().to_string();
    redis::AsyncCommands::set_ex::<_, _, ()>(
        &mut conn,
        format!("{OAUTH_STATE_PREFIX}{state}"),
        data.to_string(),
        600,
    )
    .await
    .expect("Failed to store the OAuth state");
    world.oauth_callback.state = Some(state);
}

#[then("I am redirected to my settings with an OAuth error")]
async fn i_am_redirected_with_an_error(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let prefix = format!("{}/settings?oauth_error=", front(app));
    let location = world.oauth_callback.location.clone().unwrap_or_default();
    assert!(
        location.starts_with(&prefix),
        "unexpected redirect: {location}"
    );
}

#[then("the connection of the other user is still not validated, and not pinned")]
async fn the_other_connection_is_untouched(world: &mut ApiWorld) {
    let id = world.oauth_callback.other_connection_id.unwrap();
    let connection = stored_connection(world, id).await;
    assert_eq!(connection.status, IntegrationConnectionStatus::Created);
    assert_eq!(connection.provider_user_id, None);
}

#[then(expr = "{provider} was not called by that callback")]
async fn the_provider_received_no_request(world: &mut ApiWorld, provider: ProviderName) {
    let app = world.app.get().await;
    let requests = provider
        .mock_server(app)
        .received_requests()
        .await
        .unwrap()
        .len();
    assert_eq!(requests, 0);
}

async fn credentials_of(world: &mut ApiWorld, connection_id: IntegrationConnectionId) -> usize {
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let credential = app
        .repository
        .get_oauth_credential(&mut transaction, connection_id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    usize::from(credential.is_some())
}

#[then("the credential of my connection is stored")]
async fn the_credential_is_stored(world: &mut ApiWorld) {
    let id = world.oauth_callback.connection_id.unwrap();
    assert_eq!(credentials_of(world, id).await, 1);
}

#[then("no credential is stored for my connection")]
async fn no_credential_is_stored(world: &mut ApiWorld) {
    let id = world.oauth_callback.connection_id.unwrap();
    assert_eq!(credentials_of(world, id).await, 0);
}

#[then("no credential is stored for the connection of the other user")]
async fn no_credential_for_the_other_user(world: &mut ApiWorld) {
    let id = world.oauth_callback.other_connection_id.unwrap();
    assert_eq!(credentials_of(world, id).await, 0);
}

#[given(
    expr = "Google Mail would accept any code and say that the account is {string}, if it were asked"
)]
async fn google_mail_would_accept_any_code(world: &mut ApiWorld, email: String) {
    let app = world.app.get().await;
    let server = &app.google_mail_mock_server;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": GOOGLE_ACCESS_TOKEN,
            "refresh_token": "google-refresh-token",
            "token_type": "Bearer",
            "expires_in": 3599,
            "scope": "https://www.googleapis.com/auth/gmail.modify"
        })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/users/me/profile"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "emailAddress": email, "messagesTotal": 42, "threadsTotal": 12, "historyId": "1234"
        })))
        .mount(server)
        .await;
}

#[then("no OAuth state is stored for my connection")]
async fn no_oauth_state_is_stored(world: &mut ApiWorld) {
    let id = world.oauth_callback.connection_id.unwrap().to_string();
    let app = world.app.get().await;
    let mut conn = app.cache.connection_manager.clone();
    let keys: Vec<String> = redis::cmd("KEYS")
        .arg(format!("{OAUTH_STATE_PREFIX}*"))
        .query_async(&mut conn)
        .await
        .expect("Failed to list OAuth states");
    for key in keys {
        let state: Option<String> = redis::AsyncCommands::get(&mut conn, &key)
            .await
            .expect("Failed to read OAuth state");
        assert!(
            !state.unwrap_or_default().contains(&id),
            "an OAuth state names my connection: {key}"
        );
    }
}

#[when(
    "the provider calls back with that state and a code, bearing my OAuth2 access token for MCP"
)]
async fn the_provider_calls_back_with_an_mcp_token(world: &mut ApiWorld) {
    let query = state_query(world, "provider-code");
    call_back(world, Caller::McpToken, query).await;
}
