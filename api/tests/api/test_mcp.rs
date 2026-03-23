use base64::prelude::*;
use http::StatusCode;
use ring::digest;
use rstest::*;
use secrecy::ExposeSecret;
use serde_json::{Value, json};
use uuid::Uuid;

use universal_inbox::{
    HasHtmlUrl,
    auth::auth_token::AuthenticationToken,
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{github::GithubConfig, todoist::TodoistConfig},
    },
    notification::{NotificationStatus, service::NotificationPatch},
    third_party::integrations::todoist::TodoistItem,
};
use universal_inbox_api::{
    configuration::Settings,
    integrations::{oauth2::NangoConnection, todoist::TodoistSyncResponse},
};

use crate::helpers::{
    TestedApp,
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{
        create_and_mock_integration_connection, nango_github_connection, nango_todoist_connection,
    },
    notification::{
        github::{create_notification_from_github_notification, sync_github_notifications},
        update_notification,
    },
    settings,
    task::todoist::{
        mock_todoist_complete_item_service, mock_todoist_get_item_service,
        mock_todoist_item_add_service, mock_todoist_sync_resources_service,
        sync_todoist_projects_response, todoist_item,
    },
};

async fn create_api_key(app: &AuthenticatedApp) -> AuthenticationToken {
    app.client
        .post(format!(
            "{}users/me/authentication-tokens",
            app.app.api_address
        ))
        .send()
        .await
        .expect("Failed to create API key")
        .json()
        .await
        .expect("Failed to deserialize API key response")
}

fn mcp_client() -> reqwest::Client {
    reqwest::Client::builder()
        .cookie_store(true)
        .build()
        .expect("Failed to build MCP client")
}

async fn mcp_call(
    client: &reqwest::Client,
    app: &TestedApp,
    token: &str,
    body: Value,
    session_id: Option<&str>,
) -> reqwest::Response {
    let mut builder = client
        .post(format!("{}mcp", app.api_address))
        .bearer_auth(token)
        .header("Accept", "application/json, text/event-stream");
    if let Some(sid) = session_id {
        builder = builder.header("Mcp-Session-Id", sid);
    }
    builder
        .json(&body)
        .send()
        .await
        .expect("Failed to execute MCP request")
}

async fn mcp_call_with_protocol_version(
    client: &reqwest::Client,
    app: &TestedApp,
    token: &str,
    body: Value,
    session_id: Option<&str>,
    protocol_version: Option<&str>,
) -> reqwest::Response {
    let mut builder = client
        .post(format!("{}mcp", app.api_address))
        .bearer_auth(token)
        .header("Accept", "application/json, text/event-stream");
    if let Some(sid) = session_id {
        builder = builder.header("Mcp-Session-Id", sid);
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

fn extract_session_id(response: &reqwest::Response) -> Option<String> {
    response
        .headers()
        .get("Mcp-Session-Id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// Initialize an MCP session and send the required `initialized` notification.
/// Returns (session_id, initialize_response_body).
async fn mcp_initialize(
    client: &reqwest::Client,
    app: &TestedApp,
    token: &str,
) -> (Option<String>, Value) {
    let initialize = mcp_call(
        client,
        app,
        token,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {
                    "name": "test-client",
                    "version": "1.0.0"
                }
            }
        }),
        None,
    )
    .await;
    assert_eq!(initialize.status(), StatusCode::OK);
    let session_id = extract_session_id(&initialize);
    let body: Value = mcp_json(initialize).await;

    // Send required `initialized` notification (per MCP spec)
    let initialized = mcp_call(
        client,
        app,
        token,
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }),
        session_id.as_deref(),
    )
    .await;
    assert_eq!(initialized.status(), StatusCode::ACCEPTED);

    (session_id, body)
}

async fn mcp_json(mut response: reqwest::Response) -> Value {
    let is_sse = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("text/event-stream"));

    if is_sse {
        // SSE streams may stay open with keep-alive. Read chunks until we find
        // a "data:" line containing a JSON-RPC response (has "jsonrpc" field).
        let mut buf = String::new();
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(10);
        loop {
            let chunk_result = tokio::time::timeout_at(deadline, response.chunk()).await;
            match chunk_result {
                Ok(Ok(Some(chunk))) => {
                    buf.push_str(&String::from_utf8_lossy(&chunk));
                    // Check if we have a complete JSON-RPC response
                    if let Some(data) = buf.lines().find_map(|line| line.strip_prefix("data: "))
                        && let Ok(json) = serde_json::from_str::<Value>(data)
                        && json.get("jsonrpc").is_some()
                    {
                        return json;
                    }
                }
                Ok(Ok(None)) => break, // stream closed
                Ok(Err(e)) => panic!("Failed to read SSE chunk: {e}"),
                Err(_) => panic!("Timeout waiting for SSE data. Buffer so far: {buf}"),
            }
        }
        // Stream closed, try to extract from what we have
        let data = buf
            .lines()
            .rev()
            .find_map(|line| line.strip_prefix("data: "))
            .unwrap_or_else(|| panic!("Expected SSE data line, got: {buf}"));
        serde_json::from_str(data)
            .unwrap_or_else(|err| panic!("Failed to parse SSE JSON: {err}. Body: {buf}"))
    } else {
        let body = response
            .text()
            .await
            .expect("Failed to read MCP response body");
        serde_json::from_str(&body)
            .unwrap_or_else(|err| panic!("Failed to parse JSON: {err}. Body: {body}"))
    }
}

mod protocol {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn initialize_and_list_tools(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;
        let api_key = create_api_key(&app).await;
        let token = api_key.jwt_token.expose_secret().0.clone();
        let client = mcp_client();

        let (session_id, body) = mcp_initialize(&client, &app.app, &token).await;
        assert_eq!(body["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(body["result"]["serverInfo"]["name"], "universal-inbox");
        assert!(
            body["result"]["capabilities"]["tools"]["listChanged"].is_null(),
            "RMCP omits listChanged when false"
        );

        let tools_list = mcp_call(
            &client,
            &app.app,
            &token,
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/list",
                "params": {}
            }),
            session_id.as_deref(),
        )
        .await;

        assert_eq!(tools_list.status(), StatusCode::OK);
        let body: Value = mcp_json(tools_list).await;
        let tools = body["result"]["tools"]
            .as_array()
            .expect("Expected tools array");
        let tool_names = tools
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_string())
            .collect::<Vec<String>>();

        assert!(tool_names.contains(&"list_notifications".to_string()));
        assert!(tool_names.contains(&"bulk_act_notifications".to_string()));
        assert!(tool_names.contains(&"create_task_from_notification".to_string()));
        assert!(tool_names.contains(&"list_tasks".to_string()));
        assert!(tool_names.contains(&"update_task".to_string()));
    }

    #[rstest]
    #[tokio::test]
    async fn rejects_missing_bearer_token(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;
        let response = reqwest::Client::new()
            .post(format!("{}mcp", app.app.api_address))
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-06-18"
                }
            }))
            .send()
            .await
            .expect("Failed to execute request");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[rstest]
    #[tokio::test]
    async fn validates_protocol_version_header(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;
        let api_key = create_api_key(&app).await;
        let token = api_key.jwt_token.expose_secret().0.clone();

        let tools_list_body = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        });

        // Without MCP-Protocol-Version header: the rmcp crate does not validate this header
        // at the transport layer, so the request succeeds. This documents a gap between the
        // MCP spec (2025-11-25 §3.4.1) — which requires clients to include the header on all
        // subsequent requests — and the current rmcp implementation.
        let client1 = mcp_client();
        let (session_id, _) = mcp_initialize(&client1, &app.app, &token).await;
        let without_header = mcp_call_with_protocol_version(
            &client1,
            &app.app,
            &token,
            tools_list_body.clone(),
            session_id.as_deref(),
            None,
        )
        .await;
        assert_eq!(
            without_header.status(),
            StatusCode::OK,
            "rmcp does not enforce MCP-Protocol-Version header (spec gap)"
        );
        let body: Value = mcp_json(without_header).await;
        assert!(
            body["result"]["tools"].is_array(),
            "tools/list should succeed without MCP-Protocol-Version header"
        );

        // With an invalid MCP-Protocol-Version header: rmcp does not validate header values,
        // so the request also succeeds. The spec mandates a 400 response here, but enforcement
        // is not implemented in the rmcp transport.
        let client2 = mcp_client();
        let (session_id, _) = mcp_initialize(&client2, &app.app, &token).await;
        let with_invalid_version = mcp_call_with_protocol_version(
            &client2,
            &app.app,
            &token,
            tools_list_body.clone(),
            session_id.as_deref(),
            Some("invalid-version"),
        )
        .await;
        assert_eq!(
            with_invalid_version.status(),
            StatusCode::OK,
            "rmcp does not reject invalid MCP-Protocol-Version header values (spec gap)"
        );

        // With the correct MCP-Protocol-Version header: request succeeds as expected.
        let client3 = mcp_client();
        let (session_id, _) = mcp_initialize(&client3, &app.app, &token).await;
        let with_correct_version = mcp_call_with_protocol_version(
            &client3,
            &app.app,
            &token,
            tools_list_body.clone(),
            session_id.as_deref(),
            Some("2025-06-18"),
        )
        .await;
        assert_eq!(with_correct_version.status(), StatusCode::OK);
        let body: Value = mcp_json(with_correct_version).await;
        assert!(
            body["result"]["tools"].is_array(),
            "tools/list should succeed with correct MCP-Protocol-Version header"
        );
    }

    #[rstest]
    #[tokio::test]
    async fn returns_protocol_errors_for_unknown_tool_and_invalid_arguments(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let api_key = create_api_key(&app).await;
        let token = api_key.jwt_token.expose_secret().0.clone();
        let client = mcp_client();

        // Each protocol error may close the MCP session, so use fresh clients/sessions
        let (session_id, _) = mcp_initialize(&client, &app.app, &token).await;
        let unknown_tool = mcp_call(
            &client,
            &app.app,
            &token,
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": "unknown_tool",
                    "arguments": {}
                }
            }),
            session_id.as_deref(),
        )
        .await;
        let body: Value = mcp_json(unknown_tool).await;
        assert_eq!(body["error"]["code"], -32602);

        let client2 = mcp_client();
        let (session_id, _) = mcp_initialize(&client2, &app.app, &token).await;
        let invalid_arguments = mcp_call(
            &client2,
            &app.app,
            &token,
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": "act_on_notification",
                    "arguments": {
                        "notification_id": Uuid::new_v4(),
                        "action": "snooze_until"
                    }
                }
            }),
            session_id.as_deref(),
        )
        .await;
        let body: Value = mcp_json(invalid_arguments).await;
        assert_eq!(body["error"]["code"], -32602);
    }
}

mod scenario {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn manage_notifications_and_tasks_via_mcp(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        sync_github_notifications: Vec<
            universal_inbox::third_party::integrations::github::GithubNotification,
        >,
        nango_github_connection: Box<NangoConnection>,
        nango_todoist_connection: Box<NangoConnection>,
        sync_todoist_projects_response: TodoistSyncResponse,
        todoist_item: Box<TodoistItem>,
    ) {
        let app = authenticated_app.await;
        let api_key = create_api_key(&app).await;
        let token = api_key.jwt_token.expose_secret().0.clone();

        let github_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            &settings,
            nango_github_connection,
            None,
            None,
        )
        .await;
        let _todoist_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
            &settings,
            nango_todoist_connection,
            None,
            None,
        )
        .await;
        mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "projects",
            &sync_todoist_projects_response,
            None,
        )
        .await;

        let notification_to_keep = create_notification_from_github_notification(
            &app.app,
            &sync_github_notifications[0],
            app.user.id,
            github_connection.id,
        )
        .await;

        let mut second_source_notification = sync_github_notifications[0].clone();
        second_source_notification.id = "987".to_string();
        let notification_to_unsubscribe = create_notification_from_github_notification(
            &app.app,
            &second_source_notification,
            app.user.id,
            github_connection.id,
        )
        .await;
        let _ = update_notification(
            &app,
            notification_to_unsubscribe.id,
            &NotificationPatch {
                status: Some(NotificationStatus::Read),
                ..Default::default()
            },
            app.user.id,
        )
        .await;

        // Helper: each MCP tool call needs a fresh session (sessions close after SSE response)
        async fn mcp_tool_call(
            app: &TestedApp,
            token: &str,
            tool_name: &str,
            arguments: Value,
        ) -> Value {
            let client = mcp_client();
            let (session_id, _) = mcp_initialize(&client, app, token).await;
            let response = mcp_call(
                &client,
                app,
                token,
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "tools/call",
                    "params": {
                        "name": tool_name,
                        "arguments": arguments
                    }
                }),
                session_id.as_deref(),
            )
            .await;
            mcp_json(response).await
        }

        let body = mcp_tool_call(
            &app.app,
            &token,
            "list_notifications",
            json!({ "trigger_sync": false }),
        )
        .await;
        assert_eq!(body["result"]["isError"], false);
        assert_eq!(
            body["result"]["structuredContent"]["content"]
                .as_array()
                .expect("Expected notifications content")
                .len(),
            2
        );

        let body = mcp_tool_call(
            &app.app,
            &token,
            "bulk_act_notifications",
            json!({
                "statuses": ["Read"],
                "sources": ["Github"],
                "action": "unsubscribe"
            }),
        )
        .await;
        assert_eq!(body["result"]["isError"], false);
        assert_eq!(body["result"]["structuredContent"]["count"], 1);
        assert_eq!(
            body["result"]["structuredContent"]["notifications"][0]["status"],
            "Unsubscribed"
        );

        mock_todoist_item_add_service(
            &app.app.todoist_mock_server,
            &todoist_item.id,
            notification_to_keep.title.clone(),
            Some(format!(
                "- [{}]({})",
                notification_to_keep.title,
                notification_to_keep.get_html_url()
            )),
            None,
            None,
            todoist_item.priority,
        )
        .await;
        mock_todoist_get_item_service(&app.app.todoist_mock_server, todoist_item.clone()).await;

        let body = mcp_tool_call(
            &app.app,
            &token,
            "create_task_from_notification",
            json!({
                "notification_id": notification_to_keep.id,
                "task_creation": {
                    "title": notification_to_keep.title,
                    "priority": 4
                }
            }),
        )
        .await;
        assert_eq!(body["result"]["isError"], false);
        let created_task_id = body["result"]["structuredContent"]["notification"]["task"]["id"]
            .as_str()
            .expect("Expected task id")
            .to_string();

        let body = mcp_tool_call(
            &app.app,
            &token,
            "list_tasks",
            json!({
                "status": "Active",
                "only_synced_tasks": false
            }),
        )
        .await;
        assert_eq!(body["result"]["isError"], false);
        assert_eq!(
            body["result"]["structuredContent"]["content"]
                .as_array()
                .expect("Expected tasks page content")
                .len(),
            1
        );

        mock_todoist_complete_item_service(&app.app.todoist_mock_server, &todoist_item.id).await;
        let body = mcp_tool_call(
            &app.app,
            &token,
            "update_task",
            json!({
                "task_id": created_task_id,
                "patch": { "status": "Done" }
            }),
        )
        .await;
        assert_eq!(body["result"]["isError"], false);
        assert_eq!(body["result"]["structuredContent"]["status"], "Done");
    }
}

mod oauth2 {
    use super::*;
    use crate::helpers::TestedApp;

    fn pkce_challenge(verifier: &str) -> String {
        let digest = digest::digest(&digest::SHA256, verifier.as_bytes());
        BASE64_URL_SAFE_NO_PAD.encode(digest.as_ref())
    }

    async fn register_oauth2_client(client: &reqwest::Client, app: &TestedApp) -> Value {
        let response = client
            .post(format!("{}oauth2/register", app.api_address))
            .json(&json!({
                "client_name": "test-mcp-client",
                "redirect_uris": ["http://localhost:12345/callback"]
            }))
            .send()
            .await
            .expect("Failed to register client");
        assert_eq!(response.status(), StatusCode::CREATED);
        response
            .json()
            .await
            .expect("Failed to parse register response")
    }

    /// Discover the MCP resource URL from the well-known endpoint, matching
    /// how a real MCP client would discover it.
    async fn discover_mcp_resource_url(app: &TestedApp) -> String {
        let body: Value = reqwest::Client::new()
            .get(format!(
                "{}/.well-known/oauth-protected-resource",
                app.app_address.trim_end_matches('/')
            ))
            .send()
            .await
            .expect("Failed to fetch protected resource metadata")
            .json()
            .await
            .unwrap();
        body["resource"]
            .as_str()
            .expect("Missing resource in metadata")
            .to_string()
    }

    async fn oauth2_authorize(
        auth_client: &reqwest::Client,
        app: &TestedApp,
        client_id: &str,
        code_challenge: &str,
    ) -> String {
        // Get an API key to authenticate the authorize request
        let api_key: AuthenticationToken = auth_client
            .post(format!("{}users/me/authentication-tokens", app.api_address))
            .send()
            .await
            .expect("Failed to create API key")
            .json()
            .await
            .expect("Failed to deserialize API key");
        let token = api_key.jwt_token.expose_secret().0.clone();

        // Discover the MCP resource URL from the well-known endpoint
        let resource_url = discover_mcp_resource_url(app).await;

        let no_redirect = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();

        let response = no_redirect
            .get(format!(
                "{}oauth2/authorize?response_type=code&client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&scope=read+write&state=test_state&resource={}",
                app.api_address,
                client_id,
                urlencoding::encode("http://localhost:12345/callback"),
                code_challenge,
                urlencoding::encode(&resource_url),
            ))
            .bearer_auth(&token)
            .send()
            .await
            .expect("Failed to authorize");

        assert_eq!(
            response.status(),
            StatusCode::FOUND,
            "Expected 302 redirect, got {}",
            response.status()
        );

        let location = response
            .headers()
            .get("location")
            .expect("Missing Location header")
            .to_str()
            .unwrap();
        let redirect_url = url::Url::parse(location).expect("Invalid redirect URL");
        let code = redirect_url
            .query_pairs()
            .find(|(k, _)| k == "code")
            .expect("Missing code in redirect")
            .1
            .to_string();
        let state = redirect_url
            .query_pairs()
            .find(|(k, _)| k == "state")
            .expect("Missing state in redirect")
            .1
            .to_string();
        assert_eq!(state, "test_state");
        code
    }

    async fn oauth2_token_exchange(
        client: &reqwest::Client,
        app: &TestedApp,
        client_id: &str,
        code: &str,
        code_verifier: &str,
    ) -> Value {
        let response = client
            .post(format!("{}oauth2/token", app.api_address))
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("client_id", client_id),
                ("redirect_uri", "http://localhost:12345/callback"),
                ("code_verifier", code_verifier),
            ])
            .send()
            .await
            .expect("Failed to exchange token");
        assert_eq!(response.status(), StatusCode::OK);
        response
            .json()
            .await
            .expect("Failed to parse token response")
    }

    #[rstest]
    #[tokio::test]
    async fn well_known_metadata_endpoints(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;
        let client = reqwest::Client::new();

        // Protected Resource Metadata (RFC 9728)
        let response = client
            .get(format!(
                "{}/.well-known/oauth-protected-resource",
                app.app.app_address.trim_end_matches('/')
            ))
            .send()
            .await
            .expect("Failed to fetch protected resource metadata");
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        assert!(body["resource"].as_str().unwrap().ends_with("/mcp"));
        assert!(!body["authorization_servers"].as_array().unwrap().is_empty());
        assert_eq!(body["bearer_methods_supported"], json!(["header"]));
        assert_eq!(body["scopes_supported"], json!(["read", "write"]));

        // Resource-specific variant
        let response = client
            .get(format!(
                "{}/.well-known/oauth-protected-resource/api/mcp",
                app.app.app_address.trim_end_matches('/')
            ))
            .send()
            .await
            .expect("Failed to fetch resource-specific metadata");
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        assert!(body["resource"].as_str().unwrap().ends_with("/mcp"));

        // Authorization Server Metadata (RFC 8414)
        let response = client
            .get(format!(
                "{}/.well-known/oauth-authorization-server",
                app.app.app_address.trim_end_matches('/')
            ))
            .send()
            .await
            .expect("Failed to fetch authorization server metadata");
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        assert!(body["authorization_endpoint"].as_str().is_some());
        assert!(body["token_endpoint"].as_str().is_some());
        assert!(body["registration_endpoint"].as_str().is_some());
        assert_eq!(body["response_types_supported"], json!(["code"]));
        assert_eq!(
            body["grant_types_supported"],
            json!(["authorization_code", "refresh_token"])
        );
        assert_eq!(body["code_challenge_methods_supported"], json!(["S256"]));
        assert_eq!(
            body["token_endpoint_auth_methods_supported"],
            json!(["none"])
        );
        assert_eq!(body["resource_indicators_supported"], true);
    }

    #[rstest]
    #[tokio::test]
    async fn mcp_returns_www_authenticate_without_token(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let response = reqwest::Client::new()
            .post(format!("{}mcp", app.app.api_address))
            .header("Accept", "application/json, text/event-stream")
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": { "protocolVersion": "2025-06-18" }
            }))
            .send()
            .await
            .expect("Failed to execute request");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let www_auth = response
            .headers()
            .get("www-authenticate")
            .expect("Missing WWW-Authenticate header")
            .to_str()
            .unwrap();
        assert!(
            www_auth.starts_with("Bearer resource_metadata="),
            "WWW-Authenticate should contain resource_metadata parameter, got: {www_auth}"
        );
        assert!(
            www_auth.contains("/.well-known/oauth-protected-resource"),
            "resource_metadata URL should point to well-known endpoint, got: {www_auth}"
        );
    }

    #[rstest]
    #[tokio::test]
    async fn mcp_get_without_session_returns_400(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;
        let api_key = create_api_key(&app).await;
        let token = api_key.jwt_token.expose_secret().0.clone();

        let response = reqwest::Client::new()
            .get(format!("{}mcp", app.app.api_address))
            .bearer_auth(&token)
            .header("Accept", "text/event-stream")
            .send()
            .await
            .expect("Failed to execute request");

        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "GET without Mcp-Session-Id must return 400, not 401"
        );
    }

    #[rstest]
    #[tokio::test]
    async fn full_oauth2_flow_to_mcp_tools(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;

        // Step 1: Dynamic client registration (unauthenticated)
        let unauthenticated_client = reqwest::Client::new();
        let registered = register_oauth2_client(&unauthenticated_client, &app.app).await;
        let client_id = registered["client_id"].as_str().unwrap();
        assert!(registered["client_name"].as_str().unwrap() == "test-mcp-client");

        // Step 2: Authorization code request (authenticated via session cookie)
        let code_verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let code_challenge = pkce_challenge(code_verifier);

        let code = oauth2_authorize(&app.client, &app.app, client_id, &code_challenge).await;

        // Step 3: Token exchange with PKCE
        let token_response = oauth2_token_exchange(
            &unauthenticated_client,
            &app.app,
            client_id,
            &code,
            code_verifier,
        )
        .await;
        assert_eq!(token_response["token_type"], "Bearer");
        assert_eq!(token_response["expires_in"], 3600);
        assert_eq!(token_response["scope"], "read write");
        let access_token = token_response["access_token"].as_str().unwrap();
        let refresh_token = token_response["refresh_token"].as_str().unwrap();

        // Step 4: Use the OAuth2 access token to connect to MCP
        let mcp = mcp_client();
        let (session_id, body) = mcp_initialize(&mcp, &app.app, access_token).await;
        assert!(session_id.is_some());
        assert_eq!(body["result"]["serverInfo"]["name"], "universal-inbox");

        // Step 5: List tools using the OAuth2 token
        let tools_list = mcp_call(
            &mcp,
            &app.app,
            access_token,
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/list",
                "params": {}
            }),
            session_id.as_deref(),
        )
        .await;
        assert_eq!(tools_list.status(), StatusCode::OK);
        let body: Value = mcp_json(tools_list).await;
        let tools = body["result"]["tools"]
            .as_array()
            .expect("Expected tools array");
        assert!(
            tools.len() >= 11,
            "Expected at least 11 tools, got {}",
            tools.len()
        );

        // Step 6: Refresh the token
        let refresh_response = unauthenticated_client
            .post(format!("{}oauth2/token", app.app.api_address))
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id", client_id),
            ])
            .send()
            .await
            .expect("Failed to refresh token");
        assert_eq!(refresh_response.status(), StatusCode::OK);
        let refreshed: Value = refresh_response.json().await.unwrap();
        let new_access_token = refreshed["access_token"].as_str().unwrap();
        assert_ne!(
            new_access_token, access_token,
            "New access token should differ"
        );

        // Step 7: Use the refreshed token to call MCP
        let mcp2 = mcp_client();
        let (session_id, _) = mcp_initialize(&mcp2, &app.app, new_access_token).await;
        assert!(session_id.is_some());
    }

    #[rstest]
    #[tokio::test]
    async fn oauth2_rejects_invalid_pkce_verifier(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;

        let unauthenticated_client = reqwest::Client::new();
        let registered = register_oauth2_client(&unauthenticated_client, &app.app).await;
        let client_id = registered["client_id"].as_str().unwrap();

        let code_verifier = "correct-verifier-value";
        let code_challenge = pkce_challenge(code_verifier);

        let code = oauth2_authorize(&app.client, &app.app, client_id, &code_challenge).await;

        // Exchange with wrong verifier
        let response = unauthenticated_client
            .post(format!("{}oauth2/token", app.app.api_address))
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("client_id", client_id),
                ("redirect_uri", "http://localhost:12345/callback"),
                ("code_verifier", "wrong-verifier-value"),
            ])
            .send()
            .await
            .expect("Failed to exchange token");
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "Token exchange with wrong PKCE verifier should fail"
        );
    }

    #[rstest]
    #[tokio::test]
    async fn oauth2_token_audience_validation(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;

        let unauthenticated_client = reqwest::Client::new();
        let registered = register_oauth2_client(&unauthenticated_client, &app.app).await;
        let client_id = registered["client_id"].as_str().unwrap();

        let code_verifier = "audience-test-verifier";
        let code_challenge = pkce_challenge(code_verifier);

        // Authorize with a different resource than the MCP endpoint
        let api_key = create_api_key(&app).await;
        let token = api_key.jwt_token.expose_secret().0.clone();
        let no_redirect = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();

        let response = no_redirect
            .get(format!(
                "{}oauth2/authorize?response_type=code&client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&resource={}",
                app.app.api_address,
                client_id,
                urlencoding::encode("http://localhost:12345/callback"),
                code_challenge,
                urlencoding::encode("https://wrong-resource.example.com"),
            ))
            .bearer_auth(&token)
            .send()
            .await
            .expect("Failed to authorize");

        assert_eq!(response.status(), StatusCode::FOUND);
        let location = response
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap();
        let redirect_url = url::Url::parse(location).expect("Invalid redirect URL");
        let code = redirect_url
            .query_pairs()
            .find(|(k, _)| k == "code")
            .expect("Missing code in redirect")
            .1
            .to_string();

        let token_response = oauth2_token_exchange(
            &unauthenticated_client,
            &app.app,
            client_id,
            &code,
            code_verifier,
        )
        .await;
        let wrong_aud_token = token_response["access_token"].as_str().unwrap();

        // MCP should reject the token because audience doesn't match
        let mcp = mcp_client();
        let response = mcp_call(
            &mcp,
            &app.app,
            wrong_aud_token,
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": { "name": "test", "version": "1.0" }
                }
            }),
            None,
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "MCP should reject tokens with wrong audience"
        );
    }

    #[rstest]
    #[tokio::test]
    async fn oauth2_authorize_redirects_unauthenticated_to_login(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;

        let no_redirect = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();

        let registered = register_oauth2_client(&no_redirect, &app.app).await;
        let client_id = registered["client_id"].as_str().unwrap();

        // Call authorize without any authentication (no Bearer token, no session cookie)
        let response = no_redirect
            .get(format!(
                "{}oauth2/authorize?response_type=code&client_id={}&redirect_uri={}&code_challenge=test&code_challenge_method=S256",
                app.app.api_address,
                client_id,
                urlencoding::encode("http://localhost:12345/callback"),
            ))
            .send()
            .await
            .expect("Failed to call authorize");

        // The server may return 302 (redirect to login) or 401/500 depending on
        // how the auth middleware handles unauthenticated GET requests.
        // With our OIDC-based auth, unauthenticated users get a 302 to login.
        if response.status() == StatusCode::FOUND {
            let location = response
                .headers()
                .get("location")
                .expect("Missing Location header")
                .to_str()
                .unwrap();
            assert!(
                location.contains("login"),
                "Unauthenticated authorize should redirect to login, got: {location}"
            );
            assert!(
                location.contains("redirect="),
                "Login redirect should include return URL, got: {location}"
            );
        } else {
            // If the auth middleware intercepts before the handler,
            // it returns 401 which is also acceptable
            assert!(
                response.status() == StatusCode::UNAUTHORIZED
                    || response.status() == StatusCode::FOUND,
                "Expected 302 or 401 for unauthenticated authorize, got: {}",
                response.status()
            );
        }
    }
}
