use cucumber::{given, then, when};
use pretty_assertions::assert_eq;
use reqwest::{Client, StatusCode, redirect::Policy};
use serde_json::{Value, json};

use crate::{
    steps::mcp_protocol::{my_token, record_http},
    world::ApiWorld,
};

pub fn no_redirect_client() -> Client {
    Client::builder().redirect(Policy::none()).build().unwrap()
}

#[given("an OAuth2 client is registered for a loopback callback")]
async fn a_client_is_registered(world: &mut ApiWorld) {
    register_loopback_client(world).await;
}

pub(super) async fn register_loopback_client(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let mut transaction = app
        .oauth2_service
        .begin()
        .await
        .expect("Failed to begin transaction for client registration");
    let client = app
        .oauth2_service
        .register_client(
            &mut transaction,
            Some("test-mcp-client".to_string()),
            vec!["http://localhost:12345/callback".to_string()],
        )
        .await
        .expect("Failed to register OAuth2 client via service");
    transaction
        .commit()
        .await
        .expect("Failed to commit OAuth2 client registration");
    world.mcp_oauth.client = Some(serde_json::to_value(&client).unwrap());
}

#[when("I try to initialize an MCP session with my session cookie only")]
async fn i_initialize_with_the_cookie(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let response = client
        .post(format!("{}mcp", app.api_address))
        .header("Accept", "application/json, text/event-stream")
        .json(&initialize_body())
        .send()
        .await
        .expect("Failed to execute request");
    record_http(world, response).await;
}

fn initialize_body() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "1.0" }
        }
    })
}

#[when(expr = "I try to initialize an MCP session from the origin {string}")]
async fn i_initialize_from_an_origin(world: &mut ApiWorld, origin: String) {
    let token = my_token(world);
    let app = world.app.get().await;
    let response = Client::new()
        .post(format!("{}mcp", app.api_address))
        .bearer_auth(token)
        .header("Accept", "application/json, text/event-stream")
        .header("Origin", origin)
        .json(&initialize_body())
        .send()
        .await
        .expect("Failed to execute request");
    record_http(world, response).await;
}

#[when("I open the event stream of MCP without a session")]
async fn i_open_the_stream_without_a_session(world: &mut ApiWorld) {
    let token = my_token(world);
    let app = world.app.get().await;
    let response = Client::new()
        .get(format!("{}mcp", app.api_address))
        .bearer_auth(token)
        .header("Accept", "text/event-stream")
        .send()
        .await
        .expect("Failed to execute request");
    record_http(world, response).await;
}

async fn register(
    world: &mut ApiWorld,
    client: Client,
    redirect_uris: &[&str],
    address: Option<&str>,
) {
    let app = world.app.get().await;
    let mut request = client
        .post(format!("{}oauth2/register", app.api_address))
        .json(&json!({ "client_name": "test-client", "redirect_uris": redirect_uris }));
    if let Some(address) = address {
        request = request.header("X-Forwarded-For", address);
    }
    let response = request
        .send()
        .await
        .expect("Failed to call /oauth2/register");
    record_http(world, response).await;
}

#[when(expr = "a visitor registers an OAuth2 client with the redirect URI {string}")]
async fn a_visitor_registers(world: &mut ApiWorld, redirect_uri: String) {
    register(world, Client::new(), &[&redirect_uri], None).await;
}

#[when(expr = "a visitor registers an OAuth2 client with the redirect URIs {string} and {string}")]
async fn a_visitor_registers_two(world: &mut ApiWorld, first: String, second: String) {
    register(world, Client::new(), &[&first, &second], None).await;
}

#[when(expr = "I register an OAuth2 client with the redirect URI {string}")]
async fn i_register(world: &mut ApiWorld, redirect_uri: String) {
    let client = world.client();
    register(world, client, &[&redirect_uri], None).await;
}

#[when(
    expr = "I register an OAuth2 client with the redirect URI {string}, from the address {string}"
)]
async fn i_register_from_an_address(world: &mut ApiWorld, redirect_uri: String, address: String) {
    let client = world.client();
    register(world, client, &[&redirect_uri], Some(&address)).await;
}

#[when(
    expr = "I keep registering OAuth2 clients with the redirect URI {string}, from the address {string}, up to {int} times, until I am refused"
)]
async fn i_keep_registering(
    world: &mut ApiWorld,
    redirect_uri: String,
    address: String,
    attempts: usize,
) {
    for _ in 0..attempts {
        let client = world.client();
        register(world, client, &[&redirect_uri], Some(&address)).await;
        if world.http.last().status == StatusCode::TOO_MANY_REQUESTS {
            break;
        }
    }
}

#[when("a visitor asks to authorize that client")]
async fn a_visitor_asks_to_authorize(world: &mut ApiWorld) {
    let client_id = world.mcp_oauth.client_id().to_string();
    let app = world.app.get().await;
    let response = no_redirect_client()
        .get(format!(
            "{}oauth2/authorize?response_type=code&client_id={}&redirect_uri={}&code_challenge=test&code_challenge_method=S256",
            app.api_address,
            client_id,
            urlencoding::encode("http://localhost:12345/callback"),
        ))
        .send()
        .await
        .expect("Failed to call authorize");
    record_http(world, response).await;
}

#[then("I was refused for too many requests")]
fn i_was_refused(world: &mut ApiWorld) {
    assert!(
        world
            .http
            .all()
            .iter()
            .any(|response| response.status == StatusCode::TOO_MANY_REQUESTS),
        "Never refused for too many requests"
    );
}

#[then("the visitor is sent to log in, with a way back to the authorization of that client")]
async fn the_visitor_is_sent_to_log_in(world: &mut ApiWorld) {
    let client_id = world.mcp_oauth.client_id().to_string();
    let app = world.app.get().await;
    let login = format!(
        "{}/login",
        app.front_base_url.as_str().trim_end_matches('/')
    );
    let location = world
        .http
        .last()
        .header("location")
        .expect("No Location header")
        .to_string();
    let url = url::Url::parse(&location).expect("Invalid redirect URL");
    assert_eq!(
        format!("{}{}", url.origin().ascii_serialization(), url.path()),
        login
    );
    let way_back = url
        .query_pairs()
        .find(|(name, _)| name == "redirect")
        .expect("The login redirect has no way back")
        .1
        .to_string();
    assert!(
        way_back.contains("/oauth2/authorize"),
        "way back is {way_back}"
    );
    assert!(way_back.contains(&client_id), "way back is {way_back}");
}
