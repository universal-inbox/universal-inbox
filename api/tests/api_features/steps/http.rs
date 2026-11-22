use std::fs;

use cucumber::{gherkin::Step, then, when};
use pretty_assertions::{assert_eq, assert_ne};
use reqwest::{Client, Method, StatusCode};

use crate::world::{ApiWorld, Response};

/// A client that keeps the raw (compressed) body and headers: no transparent decompression.
fn raw_client() -> Client {
    Client::builder()
        .no_gzip()
        .no_brotli()
        .no_zstd()
        .no_deflate()
        .build()
        .expect("Failed to build the HTTP client")
}

async fn send(
    world: &mut ApiWorld,
    method: &str,
    path: &str,
    headers: &[(String, String)],
    accept_encoding: Option<&str>,
) -> Response {
    let method = Method::from_bytes(method.as_bytes())
        .unwrap_or_else(|err| panic!("Unknown HTTP method {method:?}: {err}"));
    let url = format!("{}{path}", world.app.get().await.app_address);
    let client = match (&world.session, accept_encoding) {
        (_, Some(_)) => raw_client(),
        (Some(session), None) => session.client.clone(),
        (None, None) => Client::new(),
    };
    let mut request = client.request(method.clone(), &url);
    for (name, value) in headers {
        request = request.header(name, value);
    }
    if let Some(encoding) = accept_encoding {
        request = request.header("accept-encoding", encoding);
    }
    let response = request
        .send()
        .await
        .unwrap_or_else(|err| panic!("Failed to send {method} {path}: {err}"));
    Response::read(response).await
}

fn header_pairs(step: &Step) -> Vec<(String, String)> {
    step.table
        .as_ref()
        .map(|table| {
            table
                .rows
                .iter()
                .map(|row| (row[0].clone(), row[1].clone()))
                .collect()
        })
        .unwrap_or_default()
}

#[when(expr = "I request {string}")]
async fn i_request(world: &mut ApiWorld, path: String) {
    let response = send(world, "GET", &path, &[], None).await;
    world.http.record(response);
}

#[when(expr = "I send a(n) {word} request to {string}( with the headers:)")]
async fn i_send_a_request(world: &mut ApiWorld, method: String, path: String, step: &Step) {
    let response = send(world, &method, &path, &header_pairs(step), None).await;
    world.http.record(response);
}

#[when(regex = r#"^I request "([^"]+)" accepting the encoding "([^"]+)"$"#)]
async fn i_request_accepting_an_encoding(world: &mut ApiWorld, path: String, encoding: String) {
    let response = send(world, "GET", &path, &[], Some(&encoding)).await;
    world.http.record(response);
}

#[when("I check the service health")]
async fn i_check_the_service_health(world: &mut ApiWorld) {
    let response = send(world, "GET", "/ping", &[], None).await;
    world.http.record(response);
}

#[when(expr = "I keep checking the service health, up to {int} times, until it is refused")]
async fn i_keep_checking_the_service_health(world: &mut ApiWorld, attempts: usize) {
    for _ in 0..attempts {
        let response = send(world, "GET", "/ping", &[], None).await;
        let refused = response.status == StatusCode::TOO_MANY_REQUESTS;
        world.http.record(response);
        if refused {
            break;
        }
    }
}

#[then(expr = "the app answers {int}")]
fn the_app_answers(world: &mut ApiWorld, status: u16) {
    assert_eq!(world.http.last().status, status);
}

#[then("the health check succeeds")]
fn the_health_check_succeeds(world: &mut ApiWorld) {
    let status = world.http.last().status;
    assert!(status.is_success(), "Unexpected status {status}");
}

#[then("the health check answers exactly:")]
fn the_health_check_answers_exactly(world: &mut ApiWorld, step: &Step) {
    let expected = step
        .docstring
        .as_ref()
        .expect("The expected answer must be given as a doc string")
        .trim();
    assert_eq!(expected, world.http.last().text());
}

#[then("a health check is refused for too many requests")]
fn a_health_check_is_refused_for_too_many_requests(world: &mut ApiWorld) {
    let responses = world.http.all();
    let last_status = responses.last().map(|response| response.status);
    assert!(
        responses
            .iter()
            .any(|response| response.status == StatusCode::TOO_MANY_REQUESTS),
        "Expected /ping to return 429 Too Many Requests once the per-IP budget was exhausted; \
         last observed status was {last_status:?}"
    );
}

#[then(regex = r#"^the response header "([^"]+)" is "([^"]*)"$"#)]
fn the_response_header_is(world: &mut ApiWorld, name: String, expected: String) {
    assert_eq!(
        world.http.last().header(&name),
        Some(expected.as_str()),
        "header {name}"
    );
}

#[then(regex = r#"^the response header "([^"]+)" is not "([^"]*)"$"#)]
fn the_response_header_is_not(world: &mut ApiWorld, name: String, unexpected: String) {
    assert_ne!(
        world.http.last().header(&name),
        Some(unexpected.as_str()),
        "header {name}"
    );
}

#[then(regex = r#"^the response has no "([^"]+)" header$"#)]
fn the_response_has_no_header(world: &mut ApiWorld, name: String) {
    assert_eq!(world.http.last().header(&name), None, "header {name}");
}

/// The expected value is a doc string, where `<identity provider URL>` stands for the address of
/// the app's identity provider.
#[then(regex = r#"^the response header "([^"]+)" is:$"#)]
async fn the_response_header_is_the_doc_string(world: &mut ApiWorld, name: String, step: &Step) {
    let identity_provider = world
        .app
        .get()
        .await
        .oidc_issuer_mock_server
        .as_ref()
        .expect("The app has no identity provider")
        .uri();
    let expected = step
        .docstring
        .as_ref()
        .expect("The expected value must be given as a doc string")
        .trim()
        .replace("<identity provider URL>", &identity_provider);
    assert_eq!(
        world.http.last().header(&name),
        Some(expected.as_str()),
        "header {name}"
    );
}

#[then("the response carries the common security headers")]
fn the_response_carries_the_common_security_headers(world: &mut ApiWorld) {
    let response = world.http.last();
    assert_eq!(response.header("x-content-type-options"), Some("nosniff"));
    assert_eq!(
        response.header("referrer-policy"),
        Some("strict-origin-when-cross-origin")
    );
}

#[then(regex = r#"^the response is encoded as "([^"]+)"$"#)]
fn the_response_is_encoded_as(world: &mut ApiWorld, encoding: String) {
    let expected = (encoding != "identity").then_some(encoding.as_str());
    assert_eq!(world.http.last().header("content-encoding"), expected);
}

#[then(regex = r#"^the response body is the file "([^"]+)"$"#)]
fn the_response_body_is_the_file(world: &mut ApiWorld, file: String) {
    let path = format!("{}/{file}", env!("CARGO_MANIFEST_DIR"));
    let expected = fs::read(&path).unwrap_or_else(|err| panic!("Cannot read {path}: {err}"));
    assert_eq!(world.http.last().body, expected);
}

#[then(regex = r#"^the response does not allow the origin "([^"]+)"$"#)]
fn the_response_does_not_allow_the_origin(world: &mut ApiWorld, origin: String) {
    assert_ne!(
        world.http.last().header("access-control-allow-origin"),
        Some(origin.as_str())
    );
}

#[then("the response does not combine a wildcard origin with credentials")]
fn the_response_does_not_combine_a_wildcard_origin_with_credentials(world: &mut ApiWorld) {
    let response = world.http.last();
    if response.header("access-control-allow-origin") == Some("*") {
        assert_ne!(
            response.header("access-control-allow-credentials"),
            Some("true"),
            "wildcard origin combined with credentials would be a CORS misconfiguration"
        );
    }
}

#[then("a response that allows an origin does not allow credentials")]
fn a_response_that_allows_an_origin_does_not_allow_credentials(world: &mut ApiWorld) {
    let response = world.http.last();
    if response.header("access-control-allow-origin").is_some() {
        assert_ne!(
            response.header("access-control-allow-credentials"),
            Some("true"),
            "MCP scope is bearer-only; credentialed CORS for arbitrary origins would \
             re-introduce the universal-inbox-bkj.12 misconfiguration"
        );
    }
}
