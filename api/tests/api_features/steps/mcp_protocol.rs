use cucumber::{Parameter, gherkin::Step, given, then, when};
use pretty_assertions::assert_eq;
use reqwest::{Client, StatusCode};
use rmcp::{
    model::InitializeRequestParams,
    transport::streamable_http_server::session::{SessionState, SessionStore},
};
use secrecy::ExposeSecret;
use serde_json::{Value, json};
use std::str::FromStr;
use uuid::Uuid;

use universal_inbox::{auth::auth_token::AuthenticationToken, notification::NotificationStatus};
use universal_inbox_api::mcp::{McpSessionOwnerStore, RedisSessionStore};

use crate::{
    helpers::TestedApp,
    steps::{
        mcp_client::{self, PROTOCOL_VERSION, initialize_params, tools_list},
        reauthentication::create_api_token,
    },
    world::ApiWorld,
};

const NO_SUCH_NOTIFICATION: &str = "00000000-0000-4000-8000-000000000000";

pub(super) fn my_token(world: &ApiWorld) -> String {
    world
        .sessions
        .api_token
        .as_ref()
        .expect("The scenario has no API token")
        .jwt_token
        .expose_secret()
        .0
        .clone()
}

pub(super) fn other_token(world: &ApiWorld) -> String {
    world
        .mcp
        .other_api_token
        .as_ref()
        .expect("The other user has no API token")
        .jwt_token
        .expose_secret()
        .0
        .clone()
}

fn my_session_id(world: &ApiWorld) -> String {
    world
        .mcp
        .session_id
        .clone()
        .expect("No MCP session is open")
}

pub(super) async fn record_http(world: &mut ApiWorld, response: reqwest::Response) {
    world.mcp.status = Some(response.status());
    world
        .http
        .record(crate::world::Response::read(response).await);
}

async fn record(world: &mut ApiWorld, response: reqwest::Response) {
    world.mcp.status = Some(response.status());
    world.mcp.body = if response.status() == StatusCode::OK {
        Some(mcp_client::json_of(response).await)
    } else {
        None
    };
}

pub(super) fn body(world: &ApiWorld) -> &Value {
    world
        .mcp
        .body
        .as_ref()
        .expect("The MCP request got no JSON answer")
}

fn tool_names(world: &ApiWorld) -> Vec<String> {
    body(world)["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("Expected a tools array, got: {}", body(world)))
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_string())
        .collect()
}

#[given("an MCP session is open with my API token")]
async fn an_mcp_session_is_open(world: &mut ApiWorld) {
    let token = my_token(world);
    let app = world.app.get().await;
    let client = mcp_client::new_client();
    let (session_id, _) = mcp_client::initialize(&client, app, &token).await;
    world.mcp.client = Some(client);
    world.mcp.session_id = Some(session_id.expect("Expected an MCP session id"));
}

#[given("the other user has an API token of their own")]
async fn the_other_user_has_an_api_token(world: &mut ApiWorld) {
    let client = world.other_session.as_ref().unwrap().client.clone();
    let app = world.app.get().await;
    let token: AuthenticationToken = create_api_token(&client, app)
        .await
        .json()
        .await
        .expect("Failed to deserialize API key response");
    world.mcp.other_api_token = Some(token);
}

#[given("my MCP session was opened on another instance of the app, which shared it through Redis")]
async fn my_session_was_opened_elsewhere(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let session_id = format!("other-pod-session-{}", Uuid::new_v4());
    let app = world.app.get().await;
    RedisSessionStore::new(app.cache.connection_manager.clone(), 60)
        .store(
            &session_id,
            &SessionState::new(
                serde_json::from_value::<InitializeRequestParams>(initialize_params())
                    .expect("valid initialize params"),
            ),
        )
        .await
        .expect("Failed to seed session store");
    McpSessionOwnerStore::new(app.cache.connection_manager.clone(), 60)
        .record_owner(&session_id, user_id)
        .await
        .expect("Failed to seed session owner");
    world.mcp.client = Some(mcp_client::new_client());
    world.mcp.session_id = Some(session_id);
}

#[when("I initialize an MCP session with my API token")]
async fn i_initialize_a_session(world: &mut ApiWorld) {
    let token = my_token(world);
    let app = world.app.get().await;
    let client = mcp_client::new_client();
    let (session_id, answer) = mcp_client::initialize(&client, app, &token).await;
    world.mcp.client = Some(client);
    world.mcp.session_id = session_id;
    world.mcp.status = Some(StatusCode::OK);
    world.mcp.body = Some(answer);
}

#[when("I try to initialize an MCP session without any token")]
async fn i_initialize_without_a_token(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let response = Client::new()
        .post(format!("{}mcp", app.api_address))
        .header("Accept", "application/json, text/event-stream")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": { "protocolVersion": PROTOCOL_VERSION }
        }))
        .send()
        .await
        .expect("Failed to execute request");
    record_http(world, response).await;
}

#[when(expr = "I try to initialize an MCP session addressed to the host {string}")]
async fn i_initialize_for_a_host(world: &mut ApiWorld, host: String) {
    initialize_for_host(world, host).await;
}

#[when("I try to initialize an MCP session addressed to the host of the front end")]
async fn i_initialize_for_the_front_host(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let host = app
        .front_base_url
        .host_str()
        .expect("front_base_url has a host")
        .to_string();
    initialize_for_host(world, host).await;
}

async fn initialize_for_host(world: &mut ApiWorld, host: String) {
    let token = my_token(world);
    let app = world.app.get().await;
    let response = Client::new()
        .post(format!("{}mcp", app.api_address))
        .bearer_auth(token)
        .header("Accept", "application/json, text/event-stream")
        .header("Host", host)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": initialize_params()
        }))
        .send()
        .await
        .expect("Failed to execute request");
    world.mcp.status = Some(response.status());
}

#[when("I list the tools of my MCP session")]
async fn i_list_the_tools(world: &mut ApiWorld) {
    let token = my_token(world);
    let session_id = my_session_id(world);
    let client = world.mcp.client.clone().unwrap();
    let app = world.app.get().await;
    let response =
        mcp_client::call(&client, app, &token, tools_list(), Some(&session_id), None).await;
    record(world, response).await;
}

async fn list_tools_of_a_new_session(world: &mut ApiWorld, version: Option<&str>) {
    let token = my_token(world);
    let app = world.app.get().await;
    let client = mcp_client::new_client();
    let (session_id, _) = mcp_client::initialize(&client, app, &token).await;
    let response = mcp_client::call(
        &client,
        app,
        &token,
        tools_list(),
        session_id.as_deref(),
        version,
    )
    .await;
    record(world, response).await;
}

#[when("I list the tools of a new MCP session, announcing no protocol version")]
async fn i_list_without_a_version(world: &mut ApiWorld) {
    list_tools_of_a_new_session(world, None).await;
}

#[when(expr = "I list the tools of a new MCP session, announcing the protocol version {string}")]
async fn i_list_with_a_version(world: &mut ApiWorld, version: String) {
    list_tools_of_a_new_session(world, Some(&version)).await;
}

#[when("the other user replays the event stream of my MCP session")]
async fn the_other_user_replays_the_stream(world: &mut ApiWorld) {
    let token = other_token(world);
    let session_id = my_session_id(world);
    let app = world.app.get().await;
    let response = Client::new()
        .get(format!("{}mcp", app.api_address))
        .bearer_auth(token)
        .header("Accept", "text/event-stream")
        .header("Mcp-Session-Id", session_id)
        .header("Last-Event-ID", "0")
        .send()
        .await
        .expect("Failed to execute request");
    world.mcp.status = Some(response.status());
}

#[when("the other user lists the tools of my MCP session")]
async fn the_other_user_lists_the_tools(world: &mut ApiWorld) {
    let token = other_token(world);
    let session_id = my_session_id(world);
    let app = world.app.get().await;
    let response = mcp_client::call(
        &mcp_client::new_client(),
        app,
        &token,
        tools_list(),
        Some(&session_id),
        None,
    )
    .await;
    world.mcp.status = Some(response.status());
}

#[when("the other user closes my MCP session")]
async fn the_other_user_closes_the_session(world: &mut ApiWorld) {
    let token = other_token(world);
    let session_id = my_session_id(world);
    let app = world.app.get().await;
    let response = Client::new()
        .delete(format!("{}mcp", app.api_address))
        .bearer_auth(token)
        .header("Mcp-Session-Id", session_id)
        .send()
        .await
        .expect("Failed to execute request");
    world.mcp.status = Some(response.status());
}

#[when("the other user lists the tools of an MCP session that nobody opened")]
async fn the_other_user_lists_an_unknown_session(world: &mut ApiWorld) {
    let token = other_token(world);
    let app = world.app.get().await;
    let response = mcp_client::call(
        &mcp_client::new_client(),
        app,
        &token,
        tools_list(),
        Some(&Uuid::new_v4().to_string()),
        None,
    )
    .await;
    world.mcp.status = Some(response.status());
}

#[when("I list the tools of an MCP session that nobody opened")]
async fn i_list_an_unknown_session(world: &mut ApiWorld) {
    let token = my_token(world);
    let app = world.app.get().await;
    let response = mcp_client::call(
        &mcp_client::new_client(),
        app,
        &token,
        tools_list(),
        Some(&format!("unknown-session-{}", Uuid::new_v4())),
        None,
    )
    .await;
    world.mcp.status = Some(response.status());
}

pub(super) async fn call_tool_in_a_new_session(world: &mut ApiWorld, name: &str, arguments: Value) {
    let token = my_token(world);
    let app = world.app.get().await;
    let answer = mcp_client::tool_call(app, &token, name, arguments).await;
    world.mcp.status = Some(StatusCode::OK);
    world.mcp.body = Some(answer);
}

#[when(expr = "I call the MCP tool {string} in a new session, with:")]
async fn i_call_a_tool(world: &mut ApiWorld, name: String, step: &Step) {
    let arguments = serde_json::from_str(
        step.docstring
            .as_ref()
            .expect("The arguments must be given as a doc string"),
    )
    .expect("The arguments must be JSON");
    call_tool_in_a_new_session(world, &name, arguments).await;
}

#[when("I update a task in a new MCP session with a title one character over the limit")]
async fn i_update_a_task_with_a_long_title(world: &mut ApiWorld) {
    let title = "a".repeat(universal_inbox::task::TASK_TITLE_MAX_LENGTH as usize + 1);
    call_tool_in_a_new_session(
        world,
        "update_task",
        json!({ "task_id": Uuid::new_v4(), "patch": { "title": title } }),
    )
    .await;
}

#[when("I list my notifications in that session")]
async fn i_list_my_notifications_in_that_session(world: &mut ApiWorld) {
    let token = my_token(world);
    let session_id = my_session_id(world);
    let client = world.mcp.client.clone().unwrap();
    let app = world.app.get().await;
    let response = mcp_client::call(
        &client,
        app,
        &token,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "list_notifications",
                "arguments": { "trigger_sync": false }
            }
        }),
        Some(&session_id),
        None,
    )
    .await;
    record(world, response).await;
}

#[then(expr = "the MCP request is answered with status {int}")]
fn the_request_is_answered_with_status(world: &mut ApiWorld, status: u16) {
    assert_eq!(
        world.mcp.status,
        Some(StatusCode::from_u16(status).unwrap())
    );
}

#[then(expr = "the MCP server speaks protocol version {string} as {string}")]
fn the_server_speaks(world: &mut ApiWorld, version: String, name: String) {
    let result = &body(world)["result"];
    assert_eq!(result["protocolVersion"], version);
    assert_eq!(result["serverInfo"]["name"], name);
}

#[then("the MCP server does not announce changes of its tool list")]
fn the_server_does_not_announce_changes(world: &mut ApiWorld) {
    assert!(body(world)["result"]["capabilities"]["tools"]["listChanged"].is_null());
}

#[then("the MCP server lists these tools:")]
fn the_server_lists_these_tools(world: &mut ApiWorld, step: &Step) {
    let listed = tool_names(world);
    for row in &step.table.as_ref().expect("A table of tools").rows {
        assert!(listed.contains(&row[0]), "{} is not in {listed:?}", row[0]);
    }
}

#[then("the MCP server lists its tools")]
fn the_server_lists_its_tools(world: &mut ApiWorld) {
    assert!(!tool_names(world).is_empty());
}

#[then("the MCP call is rejected as invalid parameters")]
fn the_call_is_rejected(world: &mut ApiWorld) {
    assert_eq!(body(world)["error"]["code"], -32602, "{}", body(world));
}

#[then(expr = "my MCP session is stored with the client name {string}")]
async fn the_session_is_stored(world: &mut ApiWorld, name: String) {
    let session_id = my_session_id(world);
    let app = world.app.get().await;
    let state = RedisSessionStore::new(app.cache.connection_manager.clone(), 60)
        .load(&session_id)
        .await
        .expect("Failed to load session")
        .expect("initialize must persist the session to the store");
    assert_eq!(state.initialize_params.client_info.name, name);
}

#[then("the answer is not an error")]
fn the_answer_is_not_an_error(world: &mut ApiWorld) {
    assert_eq!(body(world)["result"]["isError"], false, "{}", body(world));
}

#[derive(Clone, Copy, Debug)]
pub enum BulkRequest {
    BothModes,
    NoMode,
    SweepWithoutConfirmation,
    SweepRefusingConfirmation,
    EmptyList,
    TooManyEntries,
    RepeatedNotification,
    SnoozeWithoutDate,
    ConfirmedSweep,
}

impl Parameter for BulkRequest {
    const REGEX: &'static str = "a request with both modes|a request with no mode|an unfiltered sweep without confirmation|an unfiltered sweep that refuses confirmation|an empty list|a list of 101 entries|a list naming the notification twice|a snooze without a date|an unfiltered sweep that confirms";
    const NAME: &'static str = "bulk_request";
}

impl FromStr for BulkRequest {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Ok(match text {
            "a request with both modes" => Self::BothModes,
            "a request with no mode" => Self::NoMode,
            "an unfiltered sweep without confirmation" => Self::SweepWithoutConfirmation,
            "an unfiltered sweep that refuses confirmation" => Self::SweepRefusingConfirmation,
            "an empty list" => Self::EmptyList,
            "a list of 101 entries" => Self::TooManyEntries,
            "a list naming the notification twice" => Self::RepeatedNotification,
            "a snooze without a date" => Self::SnoozeWithoutDate,
            "an unfiltered sweep that confirms" => Self::ConfirmedSweep,
            other => return Err(format!("Unknown bulk request {other:?}")),
        })
    }
}

impl BulkRequest {
    fn arguments(self, notification_id: Uuid) -> Value {
        let entry = |action: &str| json!({ "notification_id": notification_id, "action": action });
        match self {
            Self::BothModes => json!({
                "mode": "filter", "action": "mark_read", "notifications": [entry("delete")]
            }),
            Self::NoMode => json!({ "notifications": [entry("delete")] }),
            Self::SweepWithoutConfirmation => json!({ "mode": "filter", "action": "delete" }),
            Self::SweepRefusingConfirmation => {
                json!({ "mode": "filter", "action": "delete", "confirm_all": false })
            }
            Self::EmptyList => json!({ "mode": "list", "notifications": [] }),
            Self::TooManyEntries => {
                let entries: Vec<Value> = (0..101)
                    .map(|_| json!({ "notification_id": Uuid::new_v4(), "action": "mark_read" }))
                    .collect();
                json!({ "mode": "list", "notifications": entries })
            }
            Self::RepeatedNotification => json!({
                "mode": "list", "notifications": [entry("mark_read"), entry("delete")]
            }),
            Self::SnoozeWithoutDate => json!({
                "mode": "list", "notifications": [entry("snooze_until")]
            }),
            Self::ConfirmedSweep => {
                json!({ "mode": "filter", "action": "mark_read", "confirm_all": true })
            }
        }
    }
}

#[when(expr = "I ask in a new MCP session to bulk act on notifications with {bulk_request}")]
async fn i_ask_to_bulk_act(world: &mut ApiWorld, request: BulkRequest) {
    let notification_id = world
        .integrations
        .github_notification
        .as_ref()
        .map_or_else(|| NO_SUCH_NOTIFICATION.parse().unwrap(), |n| n.id.0);
    call_tool_in_a_new_session(
        world,
        "bulk_act_notifications",
        request.arguments(notification_id),
    )
    .await;
}

#[then("the MCP call is rejected as invalid arguments")]
fn the_call_is_rejected_as_invalid_arguments(world: &mut ApiWorld) {
    let answer = body(world);
    assert!(
        answer["error"]["code"] == -32602 || answer["result"]["isError"] == true,
        "expected a rejection, got: {answer}"
    );
}

#[then("my GitHub notification is still unread and not snoozed")]
async fn my_notification_is_untouched(world: &mut ApiWorld) {
    let id = world.integrations.github_notification.as_ref().unwrap().id;
    let token = my_token(world);
    let app = world.app.get().await;
    let answer = get_notification(app, &token, id.0).await;
    assert_eq!(answer["result"]["isError"], false);
    assert_eq!(
        answer["result"]["structuredContent"]["status"],
        json!(NotificationStatus::Unread)
    );
    assert!(answer["result"]["structuredContent"]["snoozed_until"].is_null());
}

async fn get_notification(app: &TestedApp, token: &str, id: Uuid) -> Value {
    mcp_client::tool_call(
        app,
        token,
        "get_notification",
        json!({ "notification_id": id }),
    )
    .await
}

#[then(expr = "that call acted on {int} notification(s)")]
fn the_call_acted_on(world: &mut ApiWorld, count: u64) {
    assert_eq!(body(world)["result"]["isError"], false, "{}", body(world));
    assert_eq!(body(world)["result"]["structuredContent"]["count"], count);
}

// Mirrors SYNC_RATE_LIMIT_PER_MINUTE in api/src/utils/rate_limit.rs
const SYNC_BUDGET: usize = 10;

#[when("I use the whole sync budget of a minute through MCP, alternating notifications and tasks")]
async fn i_use_the_sync_budget(world: &mut ApiWorld) {
    for call in 0..SYNC_BUDGET {
        let tool = if call % 2 == 0 {
            "sync_notifications"
        } else {
            "sync_tasks"
        };
        call_tool_in_a_new_session(world, tool, json!({})).await;
        assert_eq!(
            body(world)["result"]["isError"],
            false,
            "call {call}: {}",
            body(world)
        );
    }
}

#[when("I ask through MCP to sync my tasks once more")]
async fn i_sync_once_more(world: &mut ApiWorld) {
    call_tool_in_a_new_session(world, "sync_tasks", json!({})).await;
}

#[then("that sync is refused for too many requests")]
fn that_sync_is_refused(world: &mut ApiWorld) {
    assert_eq!(body(world)["result"]["isError"], true, "{}", body(world));
    assert!(
        body(world)["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .contains("Too many requests"),
        "{}",
        body(world)
    );
}
