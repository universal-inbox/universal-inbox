use base64::prelude::*;
use http::StatusCode;
use ring::digest;
use rstest::*;
use secrecy::ExposeSecret;
use serde_json::{Value, json};
use uuid::Uuid;

use universal_inbox::{
    auth::auth_token::AuthenticationToken,
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::github::GithubConfig,
    },
};
use universal_inbox_api::configuration::Settings;

use crate::helpers::integration_connection::OAuthCredentialFixture;
use crate::helpers::{
    TestedApp,
    auth::{AuthenticatedApp, authenticate_user, authenticated_app},
    integration_connection::{create_and_mock_integration_connection, github_oauth_credential},
    notification::github::create_notification_from_github_notification,
    settings,
};

async fn create_api_key(app: &AuthenticatedApp) -> AuthenticationToken {
    app.client
        .post(format!(
            "{}users/me/authentication-tokens",
            app.app.api_address
        ))
        .header(
            reqwest::header::ORIGIN,
            crate::helpers::user::front_origin_header(&app.app),
        )
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

/// Each MCP tool call needs a fresh session (sessions close after the SSE
/// response), so re-initialize before every `tools/call`.
async fn mcp_tool_call(app: &TestedApp, token: &str, tool_name: &str, arguments: Value) -> Value {
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

mod oauth2 {
    use universal_inbox::user::UserId;

    use super::*;
    use crate::helpers::TestedApp;

    fn pkce_challenge(verifier: &str) -> String {
        let digest = digest::digest(&digest::SHA256, verifier.as_bytes());
        BASE64_URL_SAFE_NO_PAD.encode(digest.as_ref())
    }

    /// Register an OAuth2 client via the service layer for test setup.
    /// The /oauth2/register HTTP endpoint requires an authenticated session,
    /// but tests for other behaviors don't care about that; they just need a
    /// client_id. Tests that exercise the HTTP endpoint itself call it
    /// directly via `app.client`.
    async fn register_oauth2_client(app: &TestedApp) -> Value {
        let mut tx = app
            .oauth2_service
            .begin()
            .await
            .expect("Failed to begin transaction for client registration");
        let client = app
            .oauth2_service
            .register_client(
                &mut tx,
                Some("test-mcp-client".to_string()),
                vec!["http://localhost:12345/callback".to_string()],
            )
            .await
            .expect("Failed to register OAuth2 client via service");
        tx.commit()
            .await
            .expect("Failed to commit OAuth2 client registration");
        serde_json::to_value(&client).expect("Failed to serialize OAuth2 client")
    }

    /// Seed an OAuth2 user-consent row through the service layer so subsequent
    /// `/authorize` calls take the "already consented" fast path instead of
    /// redirecting to the consent screen.
    async fn seed_oauth2_consent(app: &TestedApp, user_id: UserId, client_id: &str, scope: &str) {
        let mut tx = app
            .oauth2_service
            .begin()
            .await
            .expect("Failed to begin transaction for consent seeding");
        app.oauth2_service
            .record_user_consent(&mut tx, user_id, client_id, scope)
            .await
            .expect("Failed to record OAuth2 user consent");
        tx.commit().await.expect("Failed to commit consent seeding");
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
        user_id: UserId,
        client_id: &str,
        code_challenge: &str,
    ) -> String {
        oauth2_authorize_with_scope(
            auth_client,
            app,
            user_id,
            client_id,
            code_challenge,
            Some("read write"),
        )
        .await
    }

    async fn oauth2_authorize_with_scope(
        auth_client: &reqwest::Client,
        app: &TestedApp,
        user_id: UserId,
        client_id: &str,
        code_challenge: &str,
        scope: Option<&str>,
    ) -> String {
        // Pre-seed the consent so the /authorize call takes the
        // "already consented" path and issues a code directly. The consent
        // flow itself is exercised by dedicated tests below.
        seed_oauth2_consent(app, user_id, client_id, "read write").await;

        // Get an API key to authenticate the authorize request
        let api_key: AuthenticationToken = auth_client
            .post(format!("{}users/me/authentication-tokens", app.api_address))
            .header(
                reqwest::header::ORIGIN,
                crate::helpers::user::front_origin_header(app),
            )
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
                "{}oauth2/authorize?response_type=code&client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256{}&state=test_state&resource={}",
                app.api_address,
                client_id,
                urlencoding::encode("http://localhost:12345/callback"),
                code_challenge,
                scope
                    .map(|scope| format!("&scope={}", urlencoding::encode(scope)))
                    .unwrap_or_default(),
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
    async fn full_oauth2_flow_to_mcp_tools(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;

        // Step 1: Dynamic client registration (unauthenticated)
        let unauthenticated_client = reqwest::Client::new();
        let registered = register_oauth2_client(&app.app).await;
        let client_id = registered["client_id"].as_str().unwrap();
        assert!(registered["client_name"].as_str().unwrap() == "test-mcp-client");

        // Step 2: Authorization code request (authenticated via session cookie)
        let code_verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let code_challenge = pkce_challenge(code_verifier);

        let code = oauth2_authorize(
            &app.client,
            &app.app,
            app.user.id,
            client_id,
            &code_challenge,
        )
        .await;

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

    /// Run the full authorize + token exchange with `scope` and return the
    /// token response.
    async fn oauth2_access_token_with_scope(app: &AuthenticatedApp, scope: Option<&str>) -> Value {
        let registered = register_oauth2_client(&app.app).await;
        let client_id = registered["client_id"].as_str().unwrap();
        let code_verifier = "scope-test-verifier-0123456789-abcdefghijklmnop";
        let code_challenge = pkce_challenge(code_verifier);
        let code = oauth2_authorize_with_scope(
            &app.client,
            &app.app,
            app.user.id,
            client_id,
            &code_challenge,
            scope,
        )
        .await;
        oauth2_token_exchange(
            &reqwest::Client::new(),
            &app.app,
            client_id,
            &code,
            code_verifier,
        )
        .await
    }

    #[rstest]
    #[tokio::test]
    async fn read_scoped_oauth2_token_cannot_call_write_tools(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let token_response = oauth2_access_token_with_scope(&app, Some("read")).await;
        assert_eq!(token_response["scope"], "read");
        let access_token = token_response["access_token"].as_str().unwrap();

        // Read tools are allowed.
        let body = mcp_tool_call(
            &app.app,
            access_token,
            "list_notifications",
            json!({ "trigger_sync": false }),
        )
        .await;
        assert_eq!(body["result"]["isError"], false, "got: {body}");

        // Every mutating tool is rejected before it runs.
        for (tool_name, arguments) in [
            (
                "bulk_act_notifications",
                json!({ "mode": "filter", "action": "delete" }),
            ),
            (
                "act_on_notification",
                json!({ "notification_id": Uuid::new_v4(), "action": "delete" }),
            ),
            (
                "update_task",
                json!({ "task_id": Uuid::new_v4(), "patch": { "status": "Done" } }),
            ),
            ("sync_notifications", json!({})),
            ("sync_tasks", json!({})),
        ] {
            let body = mcp_tool_call(&app.app, access_token, tool_name, arguments).await;
            assert_eq!(
                body["error"]["code"], -32600,
                "{tool_name} must be rejected for a read-scoped token, got: {body}"
            );
            assert_eq!(
                body["error"]["data"]["error"], "insufficient_scope",
                "{tool_name}: {body}"
            );
        }
    }

    #[rstest]
    #[tokio::test]
    async fn read_write_scoped_oauth2_token_can_call_write_tools(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let token_response = oauth2_access_token_with_scope(&app, Some("read write")).await;
        let access_token = token_response["access_token"].as_str().unwrap();

        let body = mcp_tool_call(
            &app.app,
            access_token,
            "bulk_act_notifications",
            json!({ "mode": "filter", "statuses": ["Read"], "action": "delete" }),
        )
        .await;
        assert_eq!(body["result"]["isError"], false, "got: {body}");
    }

    #[rstest]
    #[tokio::test]
    async fn oauth2_mcp_changes_are_recorded_and_searchable_in_the_audit_log(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        let github_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            &settings,
            github_oauth_credential,
            None,
            None,
        )
        .await;
        let notification = create_notification_from_github_notification(
            &app.app,
            &crate::helpers::notification::github::github_notification(),
            app.user.id,
            github_integration_connection.id,
        )
        .await;
        let token_response = oauth2_access_token_with_scope(&app, Some("read write")).await;
        let access_token = token_response["access_token"].as_str().unwrap();

        let body = mcp_tool_call(
            &app.app,
            access_token,
            "act_on_notification",
            json!({
                "notification_id": notification.id,
                "action": "snooze_until",
                "snoozed_until": "2064-01-01T09:00:00Z"
            }),
        )
        .await;
        assert_eq!(body["result"]["isError"], false, "got: {body}");

        // A read scope is enough to search the audit log
        let read_token_response = oauth2_access_token_with_scope(&app, Some("read")).await;
        let body = mcp_tool_call(
            &app.app,
            read_token_response["access_token"].as_str().unwrap(),
            "search_audit_log",
            json!({ "entity_id": notification.id, "actor_kinds": ["Mcp"] }),
        )
        .await;
        assert_eq!(body["result"]["isError"], false, "got: {body}");
        let result = &body["result"]["structuredContent"];
        assert_eq!(result["total"], 1, "got: {body}");
        let event = &result["events"][0];
        assert_eq!(event["actor"]["kind"], "Mcp");
        assert!(
            event["actor"]["client_id"]
                .as_str()
                .is_some_and(|client_id| !client_id.is_empty()),
            "got: {body}"
        );
        assert_eq!(event["actor_name"], "test-mcp-client");
        assert_eq!(
            event["changes"]["snoozed_until"]["to"],
            "2064-01-01T09:00:00"
        );

        // Another user does not see these events
        let (other_user_client, _) =
            authenticate_user(&app.app, "5678", "Jane", "Roe", "jane@example.com").await;
        let other_api_key: AuthenticationToken = other_user_client
            .post(format!(
                "{}users/me/authentication-tokens",
                app.app.api_address
            ))
            .header(
                reqwest::header::ORIGIN,
                crate::helpers::user::front_origin_header(&app.app),
            )
            .send()
            .await
            .expect("Failed to create API key")
            .json()
            .await
            .expect("Failed to deserialize API key response");
        let body = mcp_tool_call(
            &app.app,
            &other_api_key.jwt_token.expose_secret().0,
            "search_audit_log",
            json!({ "entity_id": notification.id }),
        )
        .await;
        assert_eq!(
            body["result"]["structuredContent"]["total"], 0,
            "got: {body}"
        );
    }

    #[rstest]
    #[tokio::test]
    async fn oauth2_token_audience_validation(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;

        let unauthenticated_client = reqwest::Client::new();
        let registered = register_oauth2_client(&app.app).await;
        let client_id = registered["client_id"].as_str().unwrap();

        let code_verifier = "audience-test-verifier";
        let code_challenge = pkce_challenge(code_verifier);

        // Pre-seed consent so /authorize issues a code immediately.
        seed_oauth2_consent(&app.app, app.user.id, client_id, "read write").await;

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

    // ------------------------------------------------------------------
    // Audience guard on non-MCP routes
    // ------------------------------------------------------------------

    #[rstest]
    #[tokio::test]
    async fn oauth2_token_rejected_on_non_mcp_route(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;
        let unauthenticated = reqwest::Client::new();
        let registered = register_oauth2_client(&app.app).await;
        let client_id = registered["client_id"].as_str().unwrap();

        let code_verifier = "audience-guard-verifier";
        let code_challenge = pkce_challenge(code_verifier);
        let code = oauth2_authorize(
            &app.client,
            &app.app,
            app.user.id,
            client_id,
            &code_challenge,
        )
        .await;
        let token_response =
            oauth2_token_exchange(&unauthenticated, &app.app, client_id, &code, code_verifier)
                .await;
        let access_token = token_response["access_token"].as_str().unwrap();

        // OAuth2 access token (carries aud=MCP) must not be usable on a
        // session-only endpoint. The audience guard middleware rejects it.
        let response = reqwest::Client::new()
            .get(format!("{}users/me", app.app.api_address))
            .bearer_auth(access_token)
            .send()
            .await
            .expect("Failed to call /users/me");
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "OAuth2 audience-scoped tokens must be rejected on non-MCP routes"
        );

        // Sanity check: the original session cookie still authenticates the user.
        let response = app
            .client
            .get(format!("{}users/me", app.app.api_address))
            .send()
            .await
            .expect("Failed to call /users/me with session");
        assert_eq!(response.status(), StatusCode::OK);
    }

    /// Count active (non-revoked) refresh tokens in the DB for a given
    /// `(client_id, user_id)` pair. Used by reuse/race tests to assert that
    /// the entire family has been killed off.
    async fn count_active_refresh_tokens(app: &TestedApp, client_id: &str, user_id: UserId) -> i64 {
        sqlx::query_scalar::<_, i64>(
            r#"
                SELECT COUNT(*)
                FROM oauth2_refresh_token
                WHERE client_id = $1
                  AND user_id = $2
                  AND revoked_at IS NULL
            "#,
        )
        .bind(client_id)
        .bind(user_id.0)
        .fetch_one(&*app.repository.pool)
        .await
        .expect("Failed to count active refresh tokens")
    }

    /// Drive an initial OAuth2 flow end-to-end and return the freshly minted
    /// `(refresh_token, client_id, user_id)`. Used as setup for race / reuse
    /// regression tests below.
    async fn mint_initial_refresh_token(app: &AuthenticatedApp) -> (String, String, UserId) {
        let registered = register_oauth2_client(&app.app).await;
        let client_id = registered["client_id"].as_str().unwrap().to_string();

        let code_verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let code_challenge = pkce_challenge(code_verifier);
        let code = oauth2_authorize(
            &app.client,
            &app.app,
            app.user.id,
            &client_id,
            &code_challenge,
        )
        .await;

        let unauthenticated_client = reqwest::Client::new();
        let token_response = oauth2_token_exchange(
            &unauthenticated_client,
            &app.app,
            &client_id,
            &code,
            code_verifier,
        )
        .await;
        let refresh_token = token_response["refresh_token"]
            .as_str()
            .expect("Missing refresh_token in token response")
            .to_string();

        (refresh_token, client_id, app.user.id)
    }

    /// Regression test for universal-inbox-bkj.27: the previous SELECT → check
    /// → UPDATE pattern allowed two concurrent `/token` requests carrying the
    /// same refresh token to both pass the SELECT before either revoke fired,
    /// resulting in two parallel sessions per leaked credential.
    ///
    /// We fire two `tokio::join!`-ed HTTP requests at `/token` with the
    /// same refresh token. After the dust settles:
    ///   1. Exactly one of them must succeed (200) and the other must fail
    ///      with 400 (`Invalid refresh token`).
    ///   2. The entire token family for `(client_id, user_id)` must be
    ///      revoked — including the new refresh token freshly minted by the
    ///      winning branch — because the losing call detected reuse and
    ///      triggered family-wide revocation per RFC 6749 §10.4 /
    ///      RFC 6819 §5.2.2.3.
    #[rstest]
    #[tokio::test]
    async fn refresh_token_rotation_is_atomic_under_concurrent_reuse(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let (refresh_token, client_id, user_id) = mint_initial_refresh_token(&app).await;

        // Sanity check: one active token in the family before the race.
        assert_eq!(
            count_active_refresh_tokens(&app.app, &client_id, user_id).await,
            1,
            "Pre-race: exactly one refresh token should be active"
        );

        let url = format!("{}oauth2/token", app.app.api_address);
        let client_a = reqwest::Client::new();
        let client_b = reqwest::Client::new();
        let form = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.as_str()),
            ("client_id", client_id.as_str()),
        ];

        // Fire both refresh requests simultaneously. With the atomic
        // UPDATE … RETURNING fix, exactly one wins; without it, both 200.
        let (res_a, res_b) = tokio::join!(
            client_a.post(&url).form(&form).send(),
            client_b.post(&url).form(&form).send(),
        );
        let res_a = res_a.expect("Request A failed");
        let res_b = res_b.expect("Request B failed");

        let mut statuses = [res_a.status(), res_b.status()];
        statuses.sort_by_key(|s| s.as_u16());
        assert_eq!(
            statuses,
            [StatusCode::OK, StatusCode::BAD_REQUEST],
            "Exactly one of the two concurrent refreshes must succeed; got {statuses:?}"
        );

        // After reuse detection, the entire family must be revoked —
        // including the new refresh token that the winning branch minted
        // before the loser's reuse-detection branch wiped the family.
        assert_eq!(
            count_active_refresh_tokens(&app.app, &client_id, user_id).await,
            0,
            "Post-race: refresh-token reuse must revoke the entire (client_id, user_id) family"
        );
    }

    /// Sequential reuse detection: after a refresh succeeds, replaying the
    /// original (now-revoked) refresh token must (a) fail with
    /// `invalid_grant`-equivalent 400 AND (b) revoke the new refresh token
    /// that the legitimate first refresh minted. This pins the
    /// family-revocation contract demanded by RFC 6819 §5.2.2.3.
    #[rstest]
    #[tokio::test]
    async fn refresh_token_reuse_revokes_entire_family(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let (original_refresh, client_id, user_id) = mint_initial_refresh_token(&app).await;

        let url = format!("{}oauth2/token", app.app.api_address);
        let http = reqwest::Client::new();

        // Step 1: legitimate first refresh succeeds.
        let ok = http
            .post(&url)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", original_refresh.as_str()),
                ("client_id", client_id.as_str()),
            ])
            .send()
            .await
            .expect("First refresh failed to send");
        assert_eq!(ok.status(), StatusCode::OK);
        let new_refresh = ok
            .json::<Value>()
            .await
            .expect("Failed to parse first refresh response")["refresh_token"]
            .as_str()
            .expect("Missing refresh_token")
            .to_string();
        assert_ne!(new_refresh, original_refresh);

        // Pre-replay: exactly the new refresh token is active.
        assert_eq!(
            count_active_refresh_tokens(&app.app, &client_id, user_id).await,
            1
        );

        // Step 2: replay the ORIGINAL (now-revoked) refresh token. This is a
        // reuse attempt — must 400, and must revoke the new refresh token too.
        let replay = http
            .post(&url)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", original_refresh.as_str()),
                ("client_id", client_id.as_str()),
            ])
            .send()
            .await
            .expect("Replay refresh failed to send");
        assert_eq!(
            replay.status(),
            StatusCode::BAD_REQUEST,
            "Replaying a revoked refresh token must be rejected"
        );

        // The whole family — including the freshly minted token from step 1 —
        // must now be revoked. A subsequent legitimate refresh with the new
        // token will also fail, confirming the family is dead.
        assert_eq!(
            count_active_refresh_tokens(&app.app, &client_id, user_id).await,
            0,
            "Reuse must revoke the entire (client_id, user_id) family, including the just-minted token"
        );

        let post_family_revoke = http
            .post(&url)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", new_refresh.as_str()),
                ("client_id", client_id.as_str()),
            ])
            .send()
            .await
            .expect("Post-family-revoke refresh failed to send");
        assert_eq!(
            post_family_revoke.status(),
            StatusCode::BAD_REQUEST,
            "After family revocation, even the previously-valid new refresh token must be rejected"
        );
    }
}
