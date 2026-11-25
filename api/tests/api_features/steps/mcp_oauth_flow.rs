use base64::{Engine, prelude::BASE64_URL_SAFE_NO_PAD};
use cucumber::{gherkin::Step, given, then, when};
use pretty_assertions::assert_eq;
use reqwest::{Client, StatusCode, redirect::Policy};
use ring::digest;
use serde_json::{Value, json};

use crate::{
    steps::{
        mcp_client::{self, tools_list},
        mcp_oauth_access::register_loopback_client,
        mcp_protocol::{body, my_token, record_http},
    },
    world::{ApiWorld, Response},
};

const CALLBACK: &str = "http://localhost:12345/callback";
const STATE: &str = "test_state";

enum Resource {
    Unspecified,
    Mcp,
    Other(String),
}

fn pkce_challenge(verifier: &str) -> String {
    let digest = digest::digest(&digest::SHA256, verifier.as_bytes());
    BASE64_URL_SAFE_NO_PAD.encode(digest.as_ref())
}

fn browser(world: &mut ApiWorld) -> Client {
    world
        .mcp_oauth
        .browser
        .get_or_insert_with(|| {
            Client::builder()
                .redirect(Policy::none())
                .cookie_store(true)
                .build()
                .unwrap()
        })
        .clone()
}

fn query_of(location: &str, name: &str) -> Option<String> {
    url::Url::parse(location)
        .ok()?
        .query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.to_string())
}

async fn mcp_resource_url(world: &mut ApiWorld) -> String {
    let app = world.app.get().await;
    let body: Value = Client::new()
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

async fn authorize(world: &mut ApiWorld, scope: Option<&str>, resource: Resource) {
    let challenge = pkce_challenge(world.mcp_oauth.verifier());
    let client_id = world.mcp_oauth.client_id().to_string();
    let bearer = my_token(world);
    let resource = match resource {
        Resource::Unspecified => None,
        Resource::Mcp => Some(mcp_resource_url(world).await),
        Resource::Other(url) => Some(url),
    };
    let browser = browser(world);
    let app = world.app.get().await;
    let mut url = format!(
        "{}oauth2/authorize?response_type=code&client_id={client_id}&redirect_uri={}&code_challenge={challenge}&code_challenge_method=S256&state={STATE}",
        app.api_address,
        urlencoding::encode(CALLBACK),
    );
    if let Some(scope) = scope {
        url.push_str(&format!("&scope={}", urlencoding::encode(scope)));
    }
    if let Some(resource) = resource {
        url.push_str(&format!("&resource={}", urlencoding::encode(&resource)));
    }
    let response = browser
        .get(url)
        .bearer_auth(bearer)
        .send()
        .await
        .expect("Failed to call /authorize");
    record_http(world, response).await;
    let location = world.http.last().header("location").map(str::to_string);
    world.mcp_oauth.code = location.as_deref().and_then(|l| query_of(l, "code"));
    world.mcp_oauth.request_id = location.as_deref().and_then(|l| query_of(l, "request_id"));
}

#[given(expr = "my PKCE verifier is {string}")]
fn my_pkce_verifier_is(world: &mut ApiWorld, verifier: String) {
    world.mcp_oauth.verifier = Some(verifier);
}

#[given(expr = "I already consented to that client for the scope {string}")]
async fn i_already_consented(world: &mut ApiWorld, scope: String) {
    let user_id = world.user().id;
    let client_id = world.mcp_oauth.client_id().to_string();
    let app = world.app.get().await;
    let mut transaction = app
        .oauth2_service
        .begin()
        .await
        .expect("Failed to begin transaction for consent seeding");
    app.oauth2_service
        .record_user_consent(&mut transaction, user_id, &client_id, &scope)
        .await
        .expect("Failed to record OAuth2 user consent");
    transaction
        .commit()
        .await
        .expect("Failed to commit consent seeding");
}

#[when("I ask to authorize that client")]
async fn i_ask_to_authorize(world: &mut ApiWorld) {
    authorize(world, Some("read write"), Resource::Unspecified).await;
}

#[when(expr = "I ask to authorize that client with the scope {string}")]
async fn i_ask_to_authorize_with_a_scope(world: &mut ApiWorld, scope: String) {
    authorize(world, Some(&scope), Resource::Unspecified).await;
}

#[when("I ask to authorize that client for the MCP resource")]
async fn i_ask_to_authorize_for_mcp(world: &mut ApiWorld) {
    authorize(world, None, Resource::Mcp).await;
}

#[when(expr = "I ask to authorize that client for the MCP resource with the scope {string}")]
async fn i_ask_to_authorize_for_mcp_with_a_scope(world: &mut ApiWorld, scope: String) {
    authorize(world, Some(&scope), Resource::Mcp).await;
}

#[when(expr = "I ask to authorize that client for the resource {string}")]
async fn i_ask_to_authorize_for_a_resource(world: &mut ApiWorld, resource: String) {
    authorize(world, None, Resource::Other(resource)).await;
}

#[when("I open the consent screen of that request")]
async fn i_open_the_consent_screen(world: &mut ApiWorld) {
    let request_id = world
        .mcp_oauth
        .request_id
        .clone()
        .expect("The authorization did not ask for a consent");
    let bearer = my_token(world);
    let browser = browser(world);
    let app = world.app.get().await;
    let response: Value = browser
        .get(format!(
            "{}oauth2/authorize/consent?request_id={}",
            app.api_address,
            urlencoding::encode(&request_id),
        ))
        .bearer_auth(bearer)
        .send()
        .await
        .expect("Failed to call GET /authorize/consent")
        .json()
        .await
        .expect("Failed to parse consent GET response");
    world.mcp_oauth.csrf_token = response["csrf_token"].as_str().map(str::to_string);
}

async fn decide(world: &mut ApiWorld, decision: &str, csrf_token: String) {
    let request_id = world
        .mcp_oauth
        .request_id
        .clone()
        .expect("The authorization did not ask for a consent");
    let bearer = my_token(world);
    let browser = browser(world);
    let app = world.app.get().await;
    let response = browser
        .post(format!("{}oauth2/authorize/consent", app.api_address))
        .bearer_auth(bearer)
        .json(&json!({
            "request_id": request_id,
            "csrf_token": csrf_token,
            "decision": decision,
        }))
        .send()
        .await
        .expect("Failed to call POST /authorize/consent");
    record_http(world, response).await;
    let answer = world.http.last();
    if answer.status == StatusCode::OK {
        let redirect: Value = answer.json();
        let redirect = redirect["redirect_url"].as_str().map(str::to_string);
        world.mcp_oauth.code = redirect.as_deref().and_then(|r| query_of(r, "code"));
        world.mcp_oauth.decision_redirect = redirect;
    }
}

#[when("I allow that request")]
async fn i_allow(world: &mut ApiWorld) {
    let csrf = world.mcp_oauth.csrf_token.clone().expect("No CSRF token");
    decide(world, "allow", csrf).await;
}

#[when("I deny that request")]
async fn i_deny(world: &mut ApiWorld) {
    let csrf = world.mcp_oauth.csrf_token.clone().expect("No CSRF token");
    decide(world, "deny", csrf).await;
}

#[when(expr = "I allow that request, with the CSRF token {string}")]
async fn i_allow_with_a_csrf_token(world: &mut ApiWorld, csrf_token: String) {
    decide(world, "allow", csrf_token).await;
}

#[then(expr = "the response header {string} does not contain {string}")]
fn the_header_does_not_contain(world: &mut ApiWorld, name: String, text: String) {
    let location = world.http.last().header(&name).unwrap_or_default();
    assert!(!location.contains(&text), "location is {location}");
}

#[then(
    expr = "the redirect of the authorization goes back to the client, with a code and the state it sent"
)]
fn the_redirect_goes_back(world: &mut ApiWorld) {
    let location = world
        .http
        .last()
        .header("location")
        .unwrap_or_default()
        .to_string();
    assert_eq!(world.http.last().status, StatusCode::FOUND);
    assert!(
        location.starts_with(&format!("{CALLBACK}?")),
        "location is {location}"
    );
    assert!(
        query_of(&location, "code").is_some(),
        "location is {location}"
    );
    assert_eq!(query_of(&location, "state").as_deref(), Some(STATE));
}

#[then("the decision redirects with an authorization code")]
fn the_decision_redirects_with_a_code(world: &mut ApiWorld) {
    assert!(
        world.mcp_oauth.code.is_some(),
        "No code in {:?}",
        world.mcp_oauth.decision_redirect
    );
}

#[then(expr = "the decision redirects with the error {string} and no authorization code")]
fn the_decision_redirects_with_an_error(world: &mut ApiWorld, error: String) {
    let redirect = world
        .mcp_oauth
        .decision_redirect
        .clone()
        .expect("The decision gave no redirect");
    assert_eq!(query_of(&redirect, "error"), Some(error));
    assert_eq!(query_of(&redirect, "code"), None);
}

async fn token_request(world: &mut ApiWorld, form: &[(&str, &str)]) -> Response {
    let app = world.app.get().await;
    let response = Client::new()
        .post(format!("{}oauth2/token", app.api_address))
        .form(form)
        .send()
        .await
        .expect("Failed to call /oauth2/token");
    Response::read(response).await
}

async fn exchange(world: &mut ApiWorld, verifier: &str) {
    let code = world.mcp_oauth.code.clone().expect("No authorization code");
    let client_id = world.mcp_oauth.client_id().to_string();
    let response = token_request(
        world,
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("client_id", &client_id),
            ("redirect_uri", CALLBACK),
            ("code_verifier", verifier),
        ],
    )
    .await;
    if response.status == StatusCode::OK {
        let tokens: Value = response.json();
        world.mcp_oauth.first_refresh_token = tokens["refresh_token"].as_str().map(str::to_string);
        world.mcp_oauth.tokens = Some(tokens);
    }
    world.mcp.status = Some(response.status);
    world.http.record(response);
}

#[when("I exchange that code with my PKCE verifier")]
async fn i_exchange_with_my_verifier(world: &mut ApiWorld) {
    let verifier = world.mcp_oauth.verifier().to_string();
    exchange(world, &verifier).await;
}

#[when(expr = "I exchange that code with the PKCE verifier {string}")]
async fn i_exchange_with_a_verifier(world: &mut ApiWorld, verifier: String) {
    exchange(world, &verifier).await;
}

#[given(expr = "I hold an OAuth2 access token for MCP with the scope {string}")]
async fn i_hold_a_token(world: &mut ApiWorld, scope: String) {
    register_loopback_client(world).await;
    world.mcp_oauth.verifier = Some("scope-test-verifier-0123456789-abcdefghijklmnop".to_string());
    i_already_consented(world, "read write".to_string()).await;
    authorize(world, Some(&scope), Resource::Mcp).await;
    assert_eq!(world.http.last().status, StatusCode::FOUND);
    let verifier = world.mcp_oauth.verifier().to_string();
    exchange(world, &verifier).await;
    assert_eq!(world.http.last().status, StatusCode::OK);
}

#[then(expr = "the token response is a bearer token that lasts {int} seconds")]
fn the_token_is_a_bearer_token(world: &mut ApiWorld, seconds: u64) {
    assert_eq!(world.http.last().status, StatusCode::OK);
    let tokens = world.mcp_oauth.tokens.as_ref().unwrap();
    assert_eq!(tokens["token_type"], "Bearer");
    assert_eq!(tokens["expires_in"], seconds);
}

#[then(expr = "the token response has the scope {string}")]
fn the_token_has_the_scope(world: &mut ApiWorld, scope: String) {
    assert_eq!(world.mcp_oauth.tokens.as_ref().unwrap()["scope"], scope);
}

#[when("I initialize an MCP session with that OAuth2 token")]
async fn i_initialize_with_the_token(world: &mut ApiWorld) {
    let token = world.mcp_oauth.token("access_token").to_string();
    let app = world.app.get().await;
    let client = mcp_client::new_client();
    let (session_id, answer) = mcp_client::initialize(&client, app, &token).await;
    world.mcp.client = Some(client);
    world.mcp.session_id = session_id;
    world.mcp.status = Some(StatusCode::OK);
    world.mcp.body = Some(answer);
}

#[when("I try to initialize an MCP session with that OAuth2 token")]
async fn i_try_to_initialize_with_the_token(world: &mut ApiWorld) {
    let token = world.mcp_oauth.token("access_token").to_string();
    let app = world.app.get().await;
    let response = mcp_client::call(
        &mcp_client::new_client(),
        app,
        &token,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": mcp_client::initialize_params()
        }),
        None,
        None,
    )
    .await;
    record_http(world, response).await;
}

#[when("I list the tools of that MCP session with that OAuth2 token")]
async fn i_list_the_tools_with_the_token(world: &mut ApiWorld) {
    let token = world.mcp_oauth.token("access_token").to_string();
    let session_id = world.mcp.session_id.clone().expect("No MCP session");
    let client = world.mcp.client.clone().unwrap();
    let app = world.app.get().await;
    let response =
        mcp_client::call(&client, app, &token, tools_list(), Some(&session_id), None).await;
    world.mcp.status = Some(response.status());
    world.mcp.body = Some(mcp_client::json_of(response).await);
}

#[then(expr = "the MCP server lists at least {int} tools")]
fn the_server_lists_at_least(world: &mut ApiWorld, count: usize) {
    let tools = body(world)["result"]["tools"]
        .as_array()
        .expect("Expected a tools array")
        .len();
    assert!(
        tools >= count,
        "Expected at least {count} tools, got {tools}"
    );
}

#[then("the MCP session gets an id")]
fn the_session_gets_an_id(world: &mut ApiWorld) {
    assert!(world.mcp.session_id.is_some());
}

async fn refresh(world: &mut ApiWorld, refresh_token: &str) -> Response {
    let client_id = world.mcp_oauth.client_id().to_string();
    token_request(
        world,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &client_id),
        ],
    )
    .await
}

fn take_new_tokens(world: &mut ApiWorld, response: &Response) {
    if response.status == StatusCode::OK {
        world.mcp_oauth.previous_access_token =
            Some(world.mcp_oauth.token("access_token").to_string());
        world.mcp_oauth.tokens = Some(response.json());
    }
}

#[when("I refresh that OAuth2 token")]
async fn i_refresh(world: &mut ApiWorld) {
    let refresh_token = world.mcp_oauth.token("refresh_token").to_string();
    let response = refresh(world, &refresh_token).await;
    take_new_tokens(world, &response);
    world.http.record(response);
}

#[when("I replay the refresh token of the first exchange")]
async fn i_replay_the_first_refresh_token(world: &mut ApiWorld) {
    let first = world.mcp_oauth.first_refresh_token.clone().unwrap();
    let response = refresh(world, &first).await;
    world.http.record(response);
}

#[when("I refresh that OAuth2 token twice at the same time")]
async fn i_refresh_twice_at_once(world: &mut ApiWorld) {
    let refresh_token = world.mcp_oauth.token("refresh_token").to_string();
    let client_id = world.mcp_oauth.client_id().to_string();
    let app = world.app.get().await;
    let url = format!("{}oauth2/token", app.api_address);
    let form = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token.as_str()),
        ("client_id", client_id.as_str()),
    ];
    let (first, second) = tokio::join!(
        Client::new().post(&url).form(&form).send(),
        Client::new().post(&url).form(&form).send(),
    );
    for response in [first, second] {
        let response = Response::read(response.expect("A refresh failed to send")).await;
        take_new_tokens(world, &response);
        world.http.record(response);
    }
}

#[when("I refresh with the latest refresh token that the app gave me")]
async fn i_refresh_with_the_latest(world: &mut ApiWorld) {
    let latest = world.mcp_oauth.token("refresh_token").to_string();
    let response = refresh(world, &latest).await;
    world.http.record(response);
}

#[then("the app gave me a new access token")]
fn the_app_gave_a_new_access_token(world: &mut ApiWorld) {
    assert_eq!(world.http.last().status, StatusCode::OK);
    assert_ne!(
        Some(world.mcp_oauth.token("access_token")),
        world.mcp_oauth.previous_access_token.as_deref()
    );
}

#[then("exactly one of the two refreshes succeeded and the other was refused with 400")]
fn exactly_one_refresh_succeeded(world: &mut ApiWorld) {
    let mut statuses: Vec<u16> = world
        .http
        .all()
        .iter()
        .rev()
        .take(2)
        .map(|response| response.status.as_u16())
        .collect();
    statuses.sort_unstable();
    assert_eq!(statuses, [200, 400]);
}

fn token_of(world: &ApiWorld) -> String {
    world.mcp_oauth.token("access_token").to_string()
}

#[when(expr = "I call the MCP tool {string} with that OAuth2 token, with:")]
async fn i_call_a_tool_with_the_token(world: &mut ApiWorld, name: String, step: &Step) {
    let arguments =
        serde_json::from_str(step.docstring.as_ref().expect("Arguments as a doc string"))
            .expect("The arguments must be JSON");
    let token = token_of(world);
    let app = world.app.get().await;
    world.mcp.body = Some(mcp_client::tool_call(app, &token, &name, arguments).await);
    world.mcp.status = Some(StatusCode::OK);
}

#[when("I call these MCP tools with that OAuth2 token:")]
async fn i_call_tools_with_the_token(world: &mut ApiWorld, step: &Step) {
    let token = token_of(world);
    let rows = step.table.as_ref().expect("A table of calls").rows.clone();
    world.mcp_oauth.refused.clear();
    for row in rows.iter().skip(1) {
        let arguments: Value = serde_json::from_str(&row[1]).expect("Arguments must be JSON");
        let app = world.app.get().await;
        let answer = mcp_client::tool_call(app, &token, &row[0], arguments).await;
        world
            .mcp_oauth
            .refused
            .push(json!({ "tool": row[0], "answer": answer }));
    }
}

#[then("each of those calls is refused for lack of scope")]
fn each_call_is_refused_for_lack_of_scope(world: &mut ApiWorld) {
    assert!(!world.mcp_oauth.refused.is_empty());
    for call in &world.mcp_oauth.refused {
        let answer = &call["answer"];
        assert_eq!(answer["error"]["code"], -32600, "{call}");
        assert_eq!(
            answer["error"]["data"]["error"], "insufficient_scope",
            "{call}"
        );
    }
}
