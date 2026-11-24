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
        IntegrationConnectionId, IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        integrations::{
            google_calendar::GoogleCalendarConfig,
            google_drive::GoogleDriveConfig,
            google_mail::{GoogleMailConfig, GoogleMailContext},
            notion::{NotionConfig, NotionContext},
            ticktick::TickTickConfig,
        },
        provider::{IntegrationConnectionContext, IntegrationProvider, IntegrationProviderKind},
    },
    notification::NotificationSourceKind,
    pii::Pii,
};
use universal_inbox_api::integrations::google_mail::GoogleMailThreadList;

use crate::{
    common::settings,
    helpers::{
        TestedApp,
        integration_connection::{create_integration_connection, google_account_id},
        notification::{
            google_mail::{
                google_mail_labels_list, mock_google_mail_labels_list_service,
                mock_google_mail_threads_list_service,
            },
            sync_notifications,
        },
        notion::{
            BLOCK_ID, DISCUSSION_ID, PAGE_ID, notion_comment_event, post_signed_notion_event,
            pushed_jobs_count,
        },
        user::front_origin_header,
    },
    steps::{integration_connections::stored_connection, provider::ProviderName},
    world::ApiWorld,
};

const GOOGLE_ACCESS_TOKEN: &str = "google_mail_test_access_token";

fn front(app: &TestedApp) -> String {
    app.front_base_url
        .as_str()
        .trim_end_matches('/')
        .to_string()
}

fn config_of(kind: IntegrationProviderKind) -> IntegrationConnectionConfig {
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

async fn authorization_url(world: &mut ApiWorld) -> url::Url {
    let id = world.oauth_callback.connection_id.unwrap();
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

async fn start_flow(
    world: &mut ApiWorld,
    kind: IntegrationProviderKind,
    provider_user_id: Option<String>,
    context: Option<IntegrationConnectionContext>,
) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let connection = create_integration_connection(
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
    .await;
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

async fn api_key(client: &Client, app: &TestedApp) -> AuthenticationToken {
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

enum Caller {
    Owner,
    OtherUser,
    Anonymous,
}

async fn call_back(world: &mut ApiWorld, caller: Caller, query: String) {
    let bearer = match caller {
        Caller::Owner => {
            let client = world.client();
            let app = world.app.get().await;
            Some(api_key(&client, app).await)
        }
        Caller::OtherUser => {
            let client = world.other_session.as_ref().unwrap().client.clone();
            let app = world.app.get().await;
            Some(api_key(&client, app).await)
        }
        Caller::Anonymous => None,
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
        request = request.bearer_auth(&token.jwt_token.expose_secret().0);
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

fn state_query(world: &ApiWorld, code: &str) -> String {
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
        .into_owned();
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

#[then("the state of that flow is still stored, as a call without session consumes nothing")]
async fn the_state_is_still_stored(world: &mut ApiWorld) {
    let state = world.oauth_callback.state.clone().unwrap();
    let app = world.app.get().await;
    let mut conn = app.cache.connection_manager.clone();
    let stored: Option<String> =
        redis::AsyncCommands::get(&mut conn, format!("universal-inbox::oauth-state::{state}"))
            .await
            .expect("Failed to read OAuth state");
    assert!(
        stored.is_some(),
        "an anonymous callback must not consume the state"
    );
}

#[then(expr = "my connection is validated, and pinned to the Google account of {string}")]
async fn the_connection_is_pinned(world: &mut ApiWorld, account: String) {
    let id = world.oauth_callback.connection_id.unwrap();
    let connection = stored_connection(world, id).await;
    assert_eq!(connection.status, IntegrationConnectionStatus::Validated);
    assert_eq!(
        connection.provider_user_id,
        Some(google_account_id(&account))
    );
}

#[when("I sync my Google Mail notifications, which has no thread")]
async fn i_sync_gmail_with_no_thread(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let server = &app.google_mail_mock_server;
    mock_google_mail_labels_list_service(server, &google_mail_labels_list()).await;
    let page_size = settings().integrations["google_mail"].page_size.unwrap();
    mock_google_mail_threads_list_service(
        server,
        None,
        page_size,
        Some(vec![GoogleMailConfig::enabled().synced_label.id]),
        &GoogleMailThreadList {
            threads: None,
            result_size_estimate: 0,
            next_page_token: None,
        },
    )
    .await;
    sync_notifications(
        &client,
        &app.api_address,
        Some(NotificationSourceKind::GoogleMail),
        false,
    )
    .await;
}

#[then(expr = "my Google Mail connection caches the address {string}")]
async fn the_connection_caches_the_address(world: &mut ApiWorld, address: String) {
    let id = world.oauth_callback.connection_id.unwrap();
    let connection = stored_connection(world, id).await;
    let IntegrationProvider::GoogleMail {
        context: Some(GoogleMailContext {
            user_email_address, ..
        }),
        ..
    } = connection.provider
    else {
        panic!("Google Mail integration connection must have a context");
    };
    assert_eq!(
        user_email_address,
        Pii::<EmailAddress>::from_str(&address).unwrap()
    );
}

#[then(
    "the Google Calendar authorization asks for the scope to read the primary calendar, and the one to write events"
)]
fn the_calendar_scopes(world: &mut ApiWorld) {
    let url = world.oauth_callback.authorization_url.clone().unwrap();
    let scope = url
        .query_pairs()
        .find(|(key, _)| key == "scope")
        .expect("Missing scope in authorization URL")
        .1
        .to_string();
    let scopes: Vec<&str> = scope.split(' ').collect();
    assert!(scopes.contains(&"https://www.googleapis.com/auth/calendar.calendars.readonly"));
    assert!(scopes.contains(&"https://www.googleapis.com/auth/calendar.events"));
}

#[then("the Notion authorization asks for a user owner, with the client id and no scope")]
fn the_notion_authorization(world: &mut ApiWorld) {
    let url = world.oauth_callback.authorization_url.clone().unwrap();
    assert_eq!(
        format!(
            "{}://{}{}",
            url.scheme(),
            url.host_str().unwrap(),
            url.path()
        ),
        "https://api.notion.com/v1/oauth/authorize"
    );
    let params: Vec<(String, String)> = url
        .query_pairs()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    assert!(params.contains(&("owner".to_string(), "user".to_string())));
    assert!(params.contains(&(
        "client_id".to_string(),
        settings().integrations["notion"].oauth_client_id.clone()
    )));
    assert!(!params.iter().any(|(key, _)| key == "scope"));
}

#[then(
    "my Notion connection is validated, pinned to the Notion user, and keeps the workspace and bot"
)]
async fn the_notion_connection_is_pinned(world: &mut ApiWorld) {
    let id = world.oauth_callback.connection_id.unwrap();
    let connection = stored_connection(world, id).await;
    assert_eq!(connection.status, IntegrationConnectionStatus::Validated);
    assert_eq!(
        connection.provider_user_id,
        Some("notion-user-id".to_string())
    );
    assert_eq!(
        connection.provider,
        IntegrationProvider::Notion {
            context: Some(NotionContext {
                workspace_id: "notion-workspace-id".to_string(),
                workspace_name: Some("Acme".to_string()),
                bot_id: "notion-bot-id".to_string(),
                user_display_name: Some(Pii::new("Jane Doe".to_string())),
                user_email_address: Some(Pii::new(
                    EmailAddress::from_str("jane.doe@example.com").unwrap()
                )),
                comments_reconciled_at: None,
            }),
            config: NotionConfig {
                sync_comments_enabled: true,
                sync_mentions_enabled: true,
            },
        }
    );
}

fn notion_event() -> Value {
    notion_comment_event(
        "notion-workspace-id",
        "comment.created",
        "comment-1",
        "notion-user-someone",
        DISCUSSION_ID,
        PAGE_ID,
        BLOCK_ID,
        &["notion-user-id"],
    )
}

#[when("Notion sends an event of that workspace")]
#[given("Notion sends an event of that workspace")]
async fn notion_sends_an_event(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    post_signed_notion_event(&client, &app.api_address, &notion_event()).await;
}

#[then(expr = "{int} Notion event(s) is/are queued")]
async fn notion_events_are_queued(world: &mut ApiWorld, count: usize) {
    let app = world.app.get().await;
    assert_eq!(pushed_jobs_count(app).await, count);
}

// The same key prefix as the service, whose constant is private.
const OAUTH_STATE_PREFIX: &str = "universal-inbox::oauth-state::";

#[given(expr = "I have a new {provider} connection, not authorized yet")]
async fn i_have_a_new_connection(world: &mut ApiWorld, provider: ProviderName) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let connection = create_integration_connection(
        app,
        user_id,
        config_of(provider.0),
        IntegrationConnectionStatus::Created,
        None,
        None,
        None,
        None,
        None,
    )
    .await;
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
    let connection = create_integration_connection(
        app,
        user_id,
        config_of(IntegrationProviderKind::GoogleMail),
        IntegrationConnectionStatus::Created,
        None,
        None,
        None,
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
    let state = "crafted-state".to_string();
    let app = world.app.get().await;
    let mut conn = app.cache.connection_manager.clone();
    redis::AsyncCommands::set_ex::<_, _, ()>(
        &mut conn,
        format!("{OAUTH_STATE_PREFIX}{state}"),
        json!({
            "integration_connection_id": other_id,
            "pkce_verifier": null,
            "provider_kind": IntegrationProviderKind::GoogleMail,
            "user_id": user_id,
        })
        .to_string(),
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

async fn credentials_of(world: &mut ApiWorld, connection_id: IntegrationConnectionId) -> i64 {
    let app = world.app.get().await;
    sqlx::query_scalar("SELECT count(*) FROM oauth_credential WHERE integration_connection_id = $1")
        .bind(connection_id.0)
        .fetch_one(&*app.repository.pool)
        .await
        .unwrap()
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
