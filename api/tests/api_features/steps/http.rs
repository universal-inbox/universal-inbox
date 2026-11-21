use cucumber::{gherkin::Step, then, when};
use reqwest::StatusCode;

use crate::world::{ApiWorld, Response};

async fn check_health(world: &mut ApiWorld) -> Response {
    let url = format!("{}/ping", world.app().await.app_address);
    let response = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .expect("Failed to execute the health check request");
    let status = response.status();
    let body = response
        .text()
        .await
        .expect("Failed to read the health check response");
    Response { status, body }
}

#[when("I check the service health")]
async fn i_check_the_service_health(world: &mut ApiWorld) {
    let response = check_health(world).await;
    world.responses.push(response);
}

#[when(expr = "I keep checking the service health, up to {int} times, until it is refused")]
async fn i_keep_checking_the_service_health(world: &mut ApiWorld, attempts: usize) {
    for _ in 0..attempts {
        let response = check_health(world).await;
        let refused = response.status == StatusCode::TOO_MANY_REQUESTS;
        world.responses.push(response);
        if refused {
            break;
        }
    }
}

#[then("the health check succeeds")]
fn the_health_check_succeeds(world: &mut ApiWorld) {
    let status = world.last_response().status;
    assert!(status.is_success(), "Unexpected status {status}");
}

#[then("the health check answers exactly:")]
fn the_health_check_answers_exactly(world: &mut ApiWorld, step: &Step) {
    let expected = step
        .docstring
        .as_ref()
        .expect("The expected answer must be given as a doc string")
        .trim();
    assert_eq!(expected, world.last_response().body);
}

#[then("a health check is refused for too many requests")]
fn a_health_check_is_refused_for_too_many_requests(world: &mut ApiWorld) {
    let last_status = world.responses.last().map(|response| response.status);
    assert!(
        world
            .responses
            .iter()
            .any(|response| response.status == StatusCode::TOO_MANY_REQUESTS),
        "Expected /ping to return 429 Too Many Requests once the per-IP budget was exhausted; \
         last observed status was {last_status:?}"
    );
}
