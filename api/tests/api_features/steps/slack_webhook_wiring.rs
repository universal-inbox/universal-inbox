use chrono::Utc;
use cucumber::{given, then, when};
use pretty_assertions::assert_eq;
use serde_json::json;
use slack_morphism::prelude::*;

use crate::{
    helpers::slack::post_unsigned_slack_event,
    state::SlackEvent,
    world::{ApiWorld, Response},
};

fn hold(world: &mut ApiWorld, event: SlackPushEvent) {
    world.slack_webhook.event = Some(SlackEvent::Message(Box::new(event)));
}

#[given(expr = "Slack sends a URL verification with the challenge {string}")]
fn slack_sends_a_url_verification(world: &mut ApiWorld, challenge: String) {
    hold(
        world,
        SlackPushEvent::UrlVerification(SlackUrlVerificationEvent { challenge }),
    );
}

#[given("Slack sends an event telling that the app is rate limited")]
fn slack_sends_a_rate_limit_event(world: &mut ApiWorld) {
    hold(
        world,
        SlackPushEvent::AppRateLimited(SlackAppRateLimitedEvent {
            team_id: "T123456".to_string(),
            minute_rate_limited: Utc::now().into(),
            api_app_id: "A123456".to_string(),
        }),
    );
}

#[given("Slack sends an event telling that a user opened the home of the app")]
fn slack_sends_an_app_home_opened_event(world: &mut ApiWorld) {
    hold(
        world,
        SlackPushEvent::EventCallback(SlackPushEventCallback {
            team_id: "T123456".into(),
            api_app_id: "A123456".into(),
            event: SlackEventCallbackBody::AppHomeOpened(SlackAppHomeOpenedEvent {
                user: "U123456".into(),
                channel: "C123456".into(),
                tab: Some("home".into()),
                view: None,
            }),
            event_id: "Ev123456".into(),
            event_time: Utc::now().into(),
            event_context: None,
            authed_users: None,
            authorizations: None,
        }),
    );
}

#[when("that event reaches the app without a signature")]
async fn the_event_reaches_the_app_unsigned(world: &mut ApiWorld) {
    let client = reqwest::Client::new();
    let app = world.app.get().await;
    let response = match world.slack_webhook.event.as_ref().expect("No Slack event") {
        SlackEvent::Message(event) => {
            post_unsigned_slack_event(&client, &app.api_address, &**event).await
        }
        SlackEvent::Edit(event) => {
            post_unsigned_slack_event(&client, &app.api_address, event).await
        }
    };
    world.http.record(Response::read(response).await);
}

#[then(expr = "Slack gets the challenge {string} back, and nothing else")]
fn slack_gets_the_challenge_back(world: &mut ApiWorld, challenge: String) {
    assert_eq!(
        world.http.last().json::<serde_json::Value>(),
        json!({ "challenge": challenge })
    );
}

#[then(expr = "the answer does not give the challenge {string} back")]
fn the_answer_does_not_give_the_challenge(world: &mut ApiWorld, challenge: String) {
    assert!(!world.http.last().text().contains(&challenge));
}
