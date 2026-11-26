use std::time::Duration;

use chrono::{TimeDelta, Utc};
use cucumber::{given, then, when};
use pretty_assertions::assert_eq;
use reqwest::{Client, StatusCode, redirect::Policy};
use serde_json::{Value, json};
use wiremock::{
    Mock, MockBuilder, ResponseTemplate,
    matchers::{any, header, method, path},
};

use universal_inbox_api::{
    repository::oauth2::{CachedClientMetadata, OAuth2Repository},
    universal_inbox::oauth2::cimd::ClientMetadataDocument,
};

use crate::{
    common::{LazyMockServer, settings},
    helpers::oauth2_cimd::{CIMD_URL, metadata_document},
    world::ApiWorld,
};

use super::{
    mcp_oauth_flow::{CALLBACK, Resource, authorization_url, browser, query_of},
    mcp_protocol::{my_token, other_token, record_http},
};

const DOCUMENT_PATH: &str = "/client.json";

fn server(world: &ApiWorld) -> &LazyMockServer {
    world
        .oauth_cimd
        .server
        .as_ref()
        .expect("The scenario has no local server")
}

fn the_fetch(world: &ApiWorld) -> MockBuilder {
    Mock::given(method("GET"))
        .and(path(DOCUMENT_PATH))
        .and(header("accept", "application/json"))
        .and(header("user-agent", "universal-inbox-cimd/1.0"))
        .and(header(
            "host",
            format!("cimd.example.test:{}", server(world).address().port()).as_str(),
        ))
}

fn document() -> ClientMetadataDocument {
    metadata_document(vec![CALLBACK.to_string()])
}

async fn cached(world: &mut ApiWorld) -> Option<CachedClientMetadata> {
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    app.repository
        .get_cimd_metadata_cache(&mut transaction, CIMD_URL)
        .await
        .unwrap()
}

#[given(
    expr = "the app fetches the metadata documents of OAuth2 clients from a local server, giving up after {int} second"
)]
fn the_app_fetches_from_a_local_server(world: &mut ApiWorld, timeout_secs: u64) {
    let server = LazyMockServer::new();
    let cimd = &mut world.app.configure().settings.oauth2.cimd;
    cimd.unguarded_fetch_address = Some(*server.address());
    cimd.timeout_secs = timeout_secs;
    world.oauth_cimd.server = Some(server);
}

#[given("a local server, where the app is not told to fetch the metadata documents")]
fn a_local_server_the_app_does_not_know(world: &mut ApiWorld) {
    world.oauth_cimd.server = Some(LazyMockServer::new());
}

#[given("an OAuth2 client identified by the URL of that metadata document")]
fn a_client_identified_by_that_url(world: &mut ApiWorld) {
    world.mcp_oauth.client = Some(json!({ "client_id": CIMD_URL }));
}

#[given(
    expr = "an OAuth2 client identified by the HTTPS URL of a metadata document on that local server, named {string}"
)]
fn a_client_identified_by_a_local_url(world: &mut ApiWorld, host: String) {
    let port = server(world).address().port();
    world.mcp_oauth.client =
        Some(json!({ "client_id": format!("https://{host}:{port}{DOCUMENT_PATH}") }));
}

#[given(expr = "the local server serves once the metadata document, to cache for {int} seconds")]
async fn the_server_serves_the_document(world: &mut ApiWorld, max_age: u64) {
    the_fetch(world)
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(document())
                .insert_header(
                    "cache-control",
                    format!("public, max-age={max_age}").as_str(),
                ),
        )
        .expect(1)
        .mount(server(world))
        .await;
}

#[given(expr = "the local server serves once the metadata document with the content type {string}")]
async fn the_server_serves_the_document_as(world: &mut ApiWorld, content_type: String) {
    the_fetch(world)
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(serde_json::to_vec(&document()).unwrap(), &content_type),
        )
        .expect(1)
        .mount(server(world))
        .await;
}

#[given(expr = "the local server serves once the metadata document after {int} seconds")]
async fn the_server_serves_the_document_late(world: &mut ApiWorld, delay: u64) {
    the_fetch(world)
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(document())
                .set_delay(Duration::from_secs(delay)),
        )
        .expect(1)
        .mount(server(world))
        .await;
}

#[given(expr = "the local server serves once a metadata document of {int} bytes")]
async fn the_server_serves_a_sized_document(world: &mut ApiWorld, size: usize) {
    let mut sized = document();
    sized.client_name = Some(String::new());
    let padding = size - serde_json::to_vec(&sized).unwrap().len();
    sized.client_name = Some("x".repeat(padding));
    let body = serde_json::to_vec(&sized).unwrap();
    assert_eq!(body.len(), size);
    the_fetch(world)
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
        .expect(1)
        .mount(server(world))
        .await;
}

#[given(expr = "the local server answers once {int} to the fetch of the metadata document")]
async fn the_server_answers_a_status(world: &mut ApiWorld, status: u16) {
    the_fetch(world)
        .respond_with(ResponseTemplate::new(status))
        .expect(1)
        .mount(server(world))
        .await;
}

#[given(
    expr = "the local server answers once to the fetch of the metadata document with a redirect to {string}, where it would serve the document but is never asked"
)]
async fn the_server_redirects(world: &mut ApiWorld, target: String) {
    the_fetch(world)
        .respond_with(ResponseTemplate::new(302).insert_header("location", target.as_str()))
        .expect(1)
        .mount(server(world))
        .await;
    Mock::given(method("GET"))
        .and(path(target))
        .respond_with(ResponseTemplate::new(200).set_body_json(document()))
        .expect(0)
        .mount(server(world))
        .await;
}

#[given("the local server is not asked anything else")]
#[given("the local server is not asked anything")]
async fn the_server_is_not_asked_anything_else(world: &mut ApiWorld) {
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(server(world))
        .await;
}

#[when("that other user asks to authorize that client")]
async fn the_other_user_asks_to_authorize(world: &mut ApiWorld) {
    let url = authorization_url(world, Some("read write"), Resource::Unspecified).await;
    let bearer = other_token(world);
    let browser = world
        .oauth_cimd
        .others_browser
        .get_or_insert_with(|| {
            Client::builder()
                .redirect(Policy::none())
                .cookie_store(true)
                .build()
                .unwrap()
        })
        .clone();
    let response = browser
        .get(url)
        .bearer_auth(bearer)
        .send()
        .await
        .expect("Failed to call /authorize");
    record_http(world, response).await;
    world.oauth_cimd.others_request_id = world
        .http
        .last()
        .header("location")
        .and_then(|location| query_of(location, "request_id"));
}

#[then(expr = "the metadata document is cached as it was served, for {int} seconds")]
async fn the_document_is_cached(world: &mut ApiWorld, ttl: i64) {
    let row = cached(world).await.expect("No metadata document is cached");
    assert_eq!(
        (row.client_id.as_str(), row.document),
        (CIMD_URL, document())
    );
    let fetched_at = row.expires_at - TimeDelta::seconds(ttl);
    let now = Utc::now();
    assert!(
        fetched_at <= now && fetched_at > now - TimeDelta::minutes(1),
        "{:?} is not {ttl} seconds after the fetch",
        row.expires_at
    );
}

#[then("no metadata document is cached for that client")]
async fn no_document_is_cached(world: &mut ApiWorld) {
    let row = cached(world).await;
    assert!(row.is_none(), "{row:?}");
}

#[then("that other user is sent to the consent screen, for a request that is not mine")]
fn the_other_user_is_sent_to_the_consent_screen(world: &mut ApiWorld) {
    let front = settings().application.front_base_url.to_string();
    let request_id = world
        .oauth_cimd
        .others_request_id
        .clone()
        .expect("The other user was not asked for a consent");
    let response = world.http.last();
    assert_eq!(response.status, StatusCode::FOUND);
    assert_eq!(
        response.header("location"),
        Some(format!("{front}oauth2/consent?request_id={request_id}").as_str())
    );
    let mine = world
        .mcp_oauth
        .request_id
        .clone()
        .expect("I was not asked for a consent");
    assert_ne!(request_id, mine);
}

async fn consent_screen(
    world: &mut ApiWorld,
    browser: Client,
    bearer: String,
    request_id: &str,
) -> (StatusCode, Value) {
    let app = world.app.get().await;
    let response = browser
        .get(format!(
            "{}oauth2/authorize/consent?request_id={}",
            app.api_address,
            urlencoding::encode(request_id),
        ))
        .bearer_auth(bearer)
        .send()
        .await
        .expect("Failed to call GET /authorize/consent");
    (response.status(), response.json().await.unwrap())
}

#[then(
    "the consent screen of that other user's request shows that client to them, and is refused to me"
)]
async fn the_other_users_consent_screen_is_theirs(world: &mut ApiWorld) {
    let request_id = world
        .oauth_cimd
        .others_request_id
        .clone()
        .expect("The other user was not asked for a consent");
    let others_browser = world.oauth_cimd.others_browser.clone().unwrap();
    let (status, theirs) = consent_screen(
        world,
        others_browser.clone(),
        other_token(world),
        &request_id,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let csrf_token = theirs["csrf_token"].as_str().unwrap_or_default();
    assert!(!csrf_token.is_empty(), "{theirs:?}");
    assert_eq!(
        theirs,
        json!({
            "request_id": request_id,
            "csrf_token": csrf_token,
            "client_id": CIMD_URL,
            "client_name": "Test CIMD Client",
            "redirect_uri": CALLBACK,
            "scope": "read write",
        })
    );

    let refusal = (
        StatusCode::FORBIDDEN,
        json!({ "message": "Forbidden access: Invalid OAuth2 consent request" }),
    );
    let my_browser = browser(world);
    let from_my_session = consent_screen(world, my_browser, my_token(world), &request_id).await;
    assert_eq!(from_my_session, refusal);
    let from_their_session =
        consent_screen(world, others_browser, my_token(world), &request_id).await;
    assert_eq!(from_their_session, refusal);
}

#[then("the local server was asked exactly what it expected")]
async fn the_server_was_asked_what_it_expected(world: &mut ApiWorld) {
    server(world).verify().await;
}
