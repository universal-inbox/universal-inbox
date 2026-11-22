use cucumber::{gherkin::Step, then, when};
use reqwest::StatusCode;

use crate::world::{ApiWorld, Response};

async fn get(world: &mut ApiWorld, path: &str) -> Response {
    let url = format!("{}{path}", world.app.get().await.app_address);
    let response = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .unwrap_or_else(|err| panic!("Failed to request {path}: {err}"));
    let status = response.status();
    let body = response
        .text()
        .await
        .unwrap_or_else(|err| panic!("Failed to read the response to {path}: {err}"));
    Response { status, body }
}

#[when(expr = "I request {string}")]
async fn i_request(world: &mut ApiWorld, path: String) {
    let response = get(world, &path).await;
    world.http.record(response);
}

#[then(expr = "the app answers {int}")]
fn the_app_answers(world: &mut ApiWorld, status: u16) {
    pretty_assertions::assert_eq!(world.http.last().status, status);
}

#[when("I check the service health")]
async fn i_check_the_service_health(world: &mut ApiWorld) {
    let response = get(world, "/ping").await;
    world.http.record(response);
}

#[when(expr = "I keep checking the service health, up to {int} times, until it is refused")]
async fn i_keep_checking_the_service_health(world: &mut ApiWorld, attempts: usize) {
    for _ in 0..attempts {
        let response = get(world, "/ping").await;
        let refused = response.status == StatusCode::TOO_MANY_REQUESTS;
        world.http.record(response);
        if refused {
            break;
        }
    }
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
    assert_eq!(expected, world.http.last().body);
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
