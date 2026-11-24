use reqwest::{Client, Response, StatusCode};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::time::{Instant, timeout_at};

use crate::helpers::TestedApp;

pub const PROTOCOL_VERSION: &str = "2025-06-18";

pub fn initialize_params() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {},
        "clientInfo": { "name": "test-client", "version": "1.0.0" }
    })
}

pub fn new_client() -> Client {
    Client::builder()
        .cookie_store(true)
        .build()
        .expect("Failed to build MCP client")
}

pub async fn call(
    client: &Client,
    app: &TestedApp,
    token: &str,
    body: Value,
    session_id: Option<&str>,
    protocol_version: Option<&str>,
) -> Response {
    let mut builder = client
        .post(format!("{}mcp", app.api_address))
        .bearer_auth(token)
        .header("Accept", "application/json, text/event-stream");
    if let Some(session_id) = session_id {
        builder = builder.header("Mcp-Session-Id", session_id);
    }
    if let Some(version) = protocol_version {
        builder = builder.header("MCP-Protocol-Version", version);
    }
    builder
        .json(&body)
        .send()
        .await
        .expect("Failed to execute MCP request")
}

pub fn session_id_of(response: &Response) -> Option<String> {
    response
        .headers()
        .get("Mcp-Session-Id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

pub async fn initialize(client: &Client, app: &TestedApp, token: &str) -> (Option<String>, Value) {
    let response = call(
        client,
        app,
        token,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": initialize_params()
        }),
        None,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let session_id = session_id_of(&response);
    let body = json_of(response).await;

    let initialized = call(
        client,
        app,
        token,
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        session_id.as_deref(),
        None,
    )
    .await;
    assert_eq!(initialized.status(), StatusCode::ACCEPTED);

    (session_id, body)
}

pub async fn json_of(mut response: Response) -> Value {
    let is_sse = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|content_type| content_type.starts_with("text/event-stream"));

    if !is_sse {
        let body = response
            .text()
            .await
            .expect("Failed to read MCP response body");
        return serde_json::from_str(&body)
            .unwrap_or_else(|err| panic!("Failed to parse JSON: {err}. Body: {body}"));
    }

    let mut buffer = String::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match timeout_at(deadline, response.chunk()).await {
            Ok(Ok(Some(chunk))) => {
                buffer.push_str(&String::from_utf8_lossy(&chunk));
                if let Some(data) = buffer.lines().find_map(|line| line.strip_prefix("data: "))
                    && let Ok(json) = serde_json::from_str::<Value>(data)
                    && json.get("jsonrpc").is_some()
                {
                    return json;
                }
            }
            Ok(Ok(None)) => break,
            Ok(Err(err)) => panic!("Failed to read SSE chunk: {err}"),
            Err(_) => panic!("Timeout waiting for SSE data. Buffer so far: {buffer}"),
        }
    }
    let data = buffer
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix("data: "))
        .unwrap_or_else(|| panic!("Expected SSE data line, got: {buffer}"));
    serde_json::from_str(data)
        .unwrap_or_else(|err| panic!("Failed to parse SSE JSON: {err}. Body: {buffer}"))
}

pub async fn tool_call(app: &TestedApp, token: &str, name: &str, arguments: Value) -> Value {
    let client = new_client();
    let (session_id, _) = initialize(&client, app, token).await;
    let response = call(
        &client,
        app,
        token,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": name, "arguments": arguments }
        }),
        session_id.as_deref(),
        None,
    )
    .await;
    json_of(response).await
}

pub fn tools_list() -> Value {
    json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} })
}
