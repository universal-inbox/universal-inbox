use base64::{Engine, engine::general_purpose::STANDARD};
use cucumber::{given, then, when};
use email_address::EmailAddress;
use pretty_assertions::assert_eq;
use reqwest::{Client, StatusCode};
use secrecy::ExposeSecret;
use serde_json::{Value, json};
use std::str::FromStr;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{body_string_contains, header, method, path, query_param},
};

use universal_inbox::{
    auth::auth_token::AuthenticationToken,
    integration_connection::{
        IntegrationConnection, IntegrationConnectionId, IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        integrations::{
            google_calendar::GoogleCalendarConfig,
            google_drive::GoogleDriveConfig,
            google_mail::{GoogleMailConfig, GoogleMailContext},
            notion::NotionConfig,
            ticktick::TickTickConfig,
        },
        provider::{IntegrationConnectionContext, IntegrationProviderKind},
    },
    pii::Pii,
    user::UserId,
};
use universal_inbox_api::universal_inbox::integration_connection::service::OAUTH_STATE_PREFIX;

use crate::{
    common::settings,
    helpers::{
        TestedApp,
        integration_connection::{create_integration_connection, google_account_id},
        user::front_origin_header,
    },
    steps::{integration_connections::stored_connection, provider::ProviderName},
    world::ApiWorld,
};

pub(super) const GOOGLE_ACCESS_TOKEN: &str = "google_mail_test_access_token";

pub(super) fn front(app: &TestedApp) -> String {
    app.front_base_url
        .as_str()
        .trim_end_matches('/')
        .to_string()
}

pub(super) fn config_of(kind: IntegrationProviderKind) -> IntegrationConnectionConfig {
    match kind {
        IntegrationProviderKind::GoogleMail => {
            IntegrationConnectionConfig::GoogleMail(GoogleMailConfig::enabled())
        }
        IntegrationProviderKind::GoogleCalendar => {
            IntegrationConnectionConfig::GoogleCalendar(GoogleCalendarConfig::enabled())
        }
        IntegrationProviderKind::GoogleDrive => {
            IntegrationConnectionConfig::GoogleDrive(GoogleDriveConfig::enabled())
        }
        IntegrationProviderKind::Notion => {
            IntegrationConnectionConfig::Notion(NotionConfig::default())
        }
        IntegrationProviderKind::TickTick => {
            IntegrationConnectionConfig::TickTick(TickTickConfig::enabled())
        }
        other => panic!("No OAuth callback scenario for {other}"),
    }
}

pub(super) async fn authorization_url(world: &mut ApiWorld) -> url::Url {
    let id = world.oauth_callback.connection_id.unwrap();
    authorization_url_of(world, id).await
}

pub(super) async fn authorization_url_of(
    world: &mut ApiWorld,
    id: IntegrationConnectionId,
) -> url::Url {
    let client = world.client();
    let app = world.app.get().await;
    let body: Value = client
        .get(format!("{}oauth/authorize-url/{id}", app.api_address))
        .send()
        .await
        .expect("Failed to start OAuth authorization")
        .json()
        .await
        .expect("Failed to parse authorize-url response");
    url::Url::parse(
        body["authorization_url"]
            .as_str()
            .expect("authorization_url"),
    )
    .expect("Invalid authorization URL")
}

pub(super) async fn new_connection(
    app: &TestedApp,
    user_id: UserId,
    kind: IntegrationProviderKind,
    context: Option<IntegrationConnectionContext>,
    provider_user_id: Option<String>,
) -> Box<IntegrationConnection> {
    create_integration_connection(
        app,
        user_id,
        config_of(kind),
        IntegrationConnectionStatus::Created,
        context,
        provider_user_id,
        None,
        None,
        None,
    )
    .await
}

pub(super) async fn start_flow(
    world: &mut ApiWorld,
    kind: IntegrationProviderKind,
    provider_user_id: Option<String>,
    context: Option<IntegrationConnectionContext>,
) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let connection = new_connection(app, user_id, kind, context, provider_user_id).await;
    world.integrations.oauth_connection = Some(connection.clone());
    world.oauth_callback.connection_id = Some(connection.id);
    let url = authorization_url(world).await;
    world.oauth_callback.state = url
        .query_pairs()
        .find(|(key, _)| key == "state")
        .map(|(_, value)| value.to_string());
    world.oauth_callback.authorization_url = Some(url);
}

#[given(expr = "I started the OAuth flow of a new {provider} connection")]
async fn i_started_the_flow(world: &mut ApiWorld, provider: ProviderName) {
    start_flow(world, provider.0, None, None).await;
}

#[given(
    expr = "I started the OAuth flow of a new {provider} connection, pinned to the Google account of {string}"
)]
async fn i_started_the_flow_of_a_pinned_connection(
    world: &mut ApiWorld,
    provider: ProviderName,
    account: String,
) {
    start_flow(world, provider.0, Some(google_account_id(&account)), None).await;
}

#[given(
    expr = "I started the OAuth flow of a new Google Mail connection, pinned to the Google account of {string} and caching that address"
)]
async fn i_started_the_flow_of_a_caching_connection(world: &mut ApiWorld, account: String) {
    let context = IntegrationConnectionContext::GoogleMail(GoogleMailContext {
        user_email_address: Pii::<EmailAddress>::from_str(&account).unwrap(),
        labels: vec![],
    });
    start_flow(
        world,
        IntegrationProviderKind::GoogleMail,
        Some(google_account_id(&account)),
        Some(context),
    )
    .await;
}

#[given(expr = "the provider exchanges the code for an access token of {provider}")]
async fn the_provider_exchanges_the_code(world: &mut ApiWorld, provider: ProviderName) {
    let app = world.app.get().await;
    let server = provider.mock_server(app);
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": GOOGLE_ACCESS_TOKEN,
            "refresh_token": "google-refresh-token",
            "token_type": "Bearer",
            "expires_in": 3599,
            "scope": "https://www.googleapis.com/auth/gmail.modify"
        })))
        .expect(1)
        .mount(server)
        .await;
}

#[given(expr = "{provider} tells that the account of the access token is {string}")]
async fn the_provider_tells_the_account(
    world: &mut ApiWorld,
    provider: ProviderName,
    email: String,
) {
    let app = world.app.get().await;
    let server = provider.mock_server(app);
    let (builder, body) = match provider.0 {
        IntegrationProviderKind::GoogleMail => (
            Mock::given(method("GET")).and(path("/users/me/profile")),
            json!({"emailAddress": email, "messagesTotal": 42, "threadsTotal": 12, "historyId": "1234"}),
        ),
        IntegrationProviderKind::GoogleCalendar => (
            Mock::given(method("GET")).and(path("/calendars/primary")),
            json!({"kind": "calendar#calendar", "id": email, "summary": email, "timeZone": "Europe/Paris"}),
        ),
        IntegrationProviderKind::GoogleDrive => (
            Mock::given(method("GET"))
                .and(path("/about"))
                .and(query_param("fields", "user(emailAddress)")),
            json!({ "user": { "emailAddress": email } }),
        ),
        other => panic!("Not a Google provider: {other}"),
    };
    builder
        .and(header(
            "authorization",
            format!("Bearer {GOOGLE_ACCESS_TOKEN}"),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(server)
        .await;
}

#[given(
    expr = "Google Mail tells that the account of the access token is {string}, once for the callback and once for the next sync"
)]
async fn gmail_tells_the_account_twice(world: &mut ApiWorld, email: String) {
    let app = world.app.get().await;
    Mock::given(method("GET"))
        .and(path("/users/me/profile"))
        .and(header(
            "authorization",
            format!("Bearer {GOOGLE_ACCESS_TOKEN}"),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "emailAddress": email, "messagesTotal": 42, "threadsTotal": 12, "historyId": "1234"
        })))
        .expect(2)
        .mount(&app.google_mail_mock_server)
        .await;
}

#[given("Google Drive fails when asked who the access token belongs to")]
async fn drive_fails_on_about(world: &mut ApiWorld) {
    let app = world.app.get().await;
    Mock::given(method("GET"))
        .and(path("/about"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&app.google_drive_mock_server)
        .await;
}

#[given(
    "Notion exchanges the code for a token of the user \"notion-user-id\" in the workspace \"notion-workspace-id\""
)]
async fn notion_exchanges_the_code(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let notion = &settings().integrations["notion"];
    Mock::given(method("POST"))
        .and(path("/v1/oauth/token"))
        .and(header(
            "authorization",
            format!(
                "Basic {}",
                STANDARD.encode(format!(
                    "{}:{}",
                    notion.oauth_client_id,
                    notion.oauth_client_secret.as_str()
                ))
            ),
        ))
        .and(body_string_contains("grant_type=authorization_code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "notion-access-token",
            "token_type": "bearer",
            "bot_id": "notion-bot-id",
            "workspace_id": "notion-workspace-id",
            "workspace_name": "Acme",
            "workspace_icon": null,
            "owner": { "type": "user", "user": {
                "object": "user", "id": "notion-user-id", "name": "Jane Doe",
                "avatar_url": null, "type": "person",
                "person": { "email": "jane.doe@example.com" }
            }},
            "duplicated_template_id": null
        })))
        .expect(1)
        .mount(&app.notion_mock_server)
        .await;
}

pub(super) async fn api_key(client: &Client, app: &TestedApp) -> AuthenticationToken {
    client
        .post(format!("{}users/me/authentication-tokens", app.api_address))
        .header(reqwest::header::ORIGIN, front_origin_header(app))
        .send()
        .await
        .expect("Failed to create API key")
        .json()
        .await
        .expect("Failed to deserialize API key response")
}

pub(super) enum Caller {
    Owner,
    OtherUser,
    Anonymous,
    McpToken,
}

pub(super) async fn call_back(world: &mut ApiWorld, caller: Caller, query: String) {
    let bearer = match caller {
        Caller::Owner => {
            let client = world.client();
            let app = world.app.get().await;
            Some(
                api_key(&client, app)
                    .await
                    .jwt_token
                    .expose_secret()
                    .0
                    .clone(),
            )
        }
        Caller::OtherUser => {
            let client = world.other_session.as_ref().unwrap().client.clone();
            let app = world.app.get().await;
            Some(
                api_key(&client, app)
                    .await
                    .jwt_token
                    .expose_secret()
                    .0
                    .clone(),
            )
        }
        Caller::Anonymous => None,
        Caller::McpToken => Some(world.mcp_oauth.token("access_token").to_string()),
    };
    let app = world.app.get().await;
    let mut request = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
        .get(format!(
            "{}/api/oauth/callback?{query}",
            app.app_address.trim_end_matches('/')
        ));
    if let Some(token) = bearer {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .expect("Failed to call /api/oauth/callback");
    world.oauth_callback.status = Some(response.status());
    world.oauth_callback.location = response
        .headers()
        .get(reqwest::header::LOCATION)
        .map(|location| location.to_str().unwrap().to_string());
}

pub(super) fn state_query(world: &ApiWorld, code: &str) -> String {
    format!(
        "state={}&code={code}",
        urlencoding::encode(world.oauth_callback.state.as_ref().unwrap())
    )
}

#[when("the provider calls back with that state and a code, for the owner of the connection")]
async fn the_provider_calls_back_as_the_owner(world: &mut ApiWorld) {
    let query = state_query(world, "provider-code");
    call_back(world, Caller::Owner, query).await;
}

#[when("the provider calls back with that state and a code, for another signed in user")]
async fn the_provider_calls_back_as_another_user(world: &mut ApiWorld) {
    let query = state_query(world, "victim-code");
    call_back(world, Caller::OtherUser, query).await;
}

#[when("the provider calls back with that state and a code, without any session")]
async fn the_provider_calls_back_anonymously(world: &mut ApiWorld) {
    let query = state_query(world, "some-code");
    call_back(world, Caller::Anonymous, query).await;
}

#[when("the provider calls back with a state nobody issued, for me")]
async fn the_provider_calls_back_with_an_unknown_state(world: &mut ApiWorld) {
    call_back(
        world,
        Caller::Owner,
        "state=does-not-exist&code=irrelevant".to_string(),
    )
    .await;
}

#[when("the provider calls back with a code and no state")]
async fn the_provider_calls_back_without_a_state(world: &mut ApiWorld) {
    call_back(world, Caller::Anonymous, "code=irrelevant".to_string()).await;
}

#[when(expr = "the provider calls back with the error {string}")]
async fn the_provider_calls_back_with_an_error(world: &mut ApiWorld, error: String) {
    call_back(
        world,
        Caller::Anonymous,
        format!("error={}", urlencoding::encode(&error)),
    )
    .await;
}

#[then(expr = "I am redirected to my settings with {string}")]
async fn i_am_redirected_to_my_settings(world: &mut ApiWorld, query: String) {
    let app = world.app.get().await;
    let expected = format!("{}/settings?{query}", front(app));
    assert_eq!(world.oauth_callback.status, Some(StatusCode::FOUND));
    assert_eq!(
        world.oauth_callback.location.as_deref(),
        Some(expected.as_str())
    );
}

#[then(expr = "that redirect does not contain {string}")]
fn the_redirect_does_not_contain(world: &mut ApiWorld, text: String) {
    let location = urlencoding::decode(world.oauth_callback.location.as_ref().unwrap())
        .unwrap()
        .replace('+', " ");
    assert!(
        !location
            .to_ascii_lowercase()
            .contains(&text.to_ascii_lowercase()),
        "redirect leaks {text:?}: {location}"
    );
}

#[then("my connection is still not validated, and not pinned to any account")]
async fn the_connection_is_untouched(world: &mut ApiWorld) {
    let id = world.oauth_callback.connection_id.unwrap();
    let connection = stored_connection(world, id).await;
    assert_eq!(connection.status, IntegrationConnectionStatus::Created);
    assert_eq!(connection.provider_user_id, None);
}

async fn stored_state(world: &mut ApiWorld) -> Option<String> {
    let state = world.oauth_callback.state.clone().unwrap();
    let app = world.app.get().await;
    let mut conn = app.cache.connection_manager.clone();
    redis::AsyncCommands::get(&mut conn, format!("{OAUTH_STATE_PREFIX}{state}"))
        .await
        .expect("Failed to read OAuth state")
}

#[then("the state of that flow is gone")]
async fn the_state_is_gone(world: &mut ApiWorld) {
    assert_eq!(stored_state(world).await, None);
}

#[then("the state of that flow is still stored")]
async fn the_state_is_still_stored(world: &mut ApiWorld) {
    let stored = stored_state(world).await;
    assert!(stored.is_some(), "the callback must not consume the state");
}
