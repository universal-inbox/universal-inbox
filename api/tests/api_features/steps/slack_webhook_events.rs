use std::str::FromStr;

use cucumber::{Parameter, given, then, when};
use pretty_assertions::assert_eq;
use redis_apalis::AsyncCommands;
use reqwest::Client;
use serde_json::{Value, json};
use slack_morphism::prelude::*;

use universal_inbox::integration_connection::{
    config::IntegrationConnectionConfig, integrations::slack::SlackConfig,
};
use universal_inbox_api::jobs::{
    UniversalInboxJob, envelope::JobEnvelope, slack::SlackPushEventCallbackJob,
};

use crate::{
    common::settings,
    helpers::{
        TestedApp,
        integration_connection::{
            create_and_mock_integration_connection, slack_context, slack_oauth_credential,
        },
        load_json_fixture_file,
        notification::slack::{
            create_notification_from_slack_thread, slack_push_message_event,
            slack_push_message_in_thread_event, slack_thread,
        },
        slack::post_signed_slack_event,
    },
    state::SlackEvent,
    world::{ApiWorld, Response},
};

#[derive(Clone, Copy, Debug)]
pub enum Subscription {
    Subscribed,
    Unsubscribed,
}

impl Subscription {
    pub fn is_subscribed(self) -> bool {
        matches!(self, Self::Subscribed)
    }
}

impl Parameter for Subscription {
    const REGEX: &'static str = "subscribed|unsubscribed";
    const NAME: &'static str = "subscription";
}

impl FromStr for Subscription {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "subscribed" => Ok(Self::Subscribed),
            "unsubscribed" => Ok(Self::Unsubscribed),
            other => Err(format!("Unknown subscription {other:?}")),
        }
    }
}

fn edit_of(world: &mut ApiWorld) -> &mut Value {
    match world.slack_webhook.event.as_mut() {
        Some(SlackEvent::Edit(event)) => event,
        _ => panic!("The scenario has no edited message"),
    }
}

fn message_of(world: &mut ApiWorld) -> &mut SlackMessageEvent {
    match world.slack_webhook.event.as_mut() {
        Some(SlackEvent::Message(event)) => match &mut **event {
            SlackPushEvent::EventCallback(SlackPushEventCallback {
                event: SlackEventCallbackBody::Message(message),
                ..
            }) => message,
            _ => panic!("Unexpected event type"),
        },
        _ => panic!("The scenario has no message"),
    }
}

#[given(expr = "I am connected to Slack, syncing its notifications, as the Slack user {string}")]
async fn i_am_connected_to_slack_as(world: &mut ApiWorld, slack_user_id: String) {
    let user_id = world.user().id;
    let mut credential = slack_oauth_credential();
    credential.provider_user_id = Some(slack_user_id);
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
        &settings(),
        credential,
        None,
        Some(slack_context("T01")),
    )
    .await;
    world.integrations.slack_connection = Some(connection);
}

#[given(expr = "I know a Slack thread that I am {subscription} to")]
async fn i_know_a_thread(world: &mut ApiWorld, subscription: Subscription) {
    let user_id = world.user().id;
    let connection_id = world
        .integrations
        .slack_connection
        .as_ref()
        .expect("The account is not connected to Slack")
        .id;
    let mut thread = slack_thread();
    thread.subscribed = subscription.is_subscribed();
    let app = world.app.get().await;
    create_notification_from_slack_thread(app, &thread, user_id, connection_id).await;
    world.slack_webhook.known_thread = Some(thread);
}

#[given("Slack sends a message in a channel")]
fn slack_sends_a_message(world: &mut ApiWorld) {
    world.slack_webhook.event = Some(SlackEvent::Message(slack_push_message_event()));
}

#[given("Slack sends a message in a thread")]
fn slack_sends_a_message_in_a_thread(world: &mut ApiWorld) {
    world.slack_webhook.event = Some(SlackEvent::Message(slack_push_message_in_thread_event()));
}

#[given("Slack sends the edit of a message")]
fn slack_sends_an_edit(world: &mut ApiWorld) {
    world.slack_webhook.event = Some(SlackEvent::Edit(load_json_fixture_file(
        "slack_push_message_changed_event.json",
    )));
}

#[given(expr = "that message mentions the Slack user {string}")]
fn that_message_mentions(world: &mut ApiWorld, slack_user_id: String) {
    let message = message_of(world);
    let content = message
        .content
        .as_mut()
        .expect("The message has no content");
    content.blocks = Some(vec![SlackBlock::RichText(
        serde_json::from_value(json!({
            "block_id": "12345",
            "elements": [{
                "type": "rich_text_section",
                "elements": [{ "type": "user", "user_id": slack_user_id }]
            }]
        }))
        .unwrap(),
    )]);
}

#[given("that message is a reply in the known thread")]
fn that_message_is_a_reply_in_the_known_thread(world: &mut ApiWorld) {
    let root = world
        .slack_webhook
        .known_thread
        .as_ref()
        .expect("No thread is known")
        .messages
        .first()
        .origin
        .ts
        .clone();
    message_of(world).origin.thread_ts = Some(root);
}

#[given("that edit mentions no one anymore")]
fn that_edit_mentions_no_one(world: &mut ApiWorld) {
    let message = &mut edit_of(world)["event"]["message"];
    message["text"] = "Hello world".into();
    message["blocks"] = json!([{
        "type": "rich_text",
        "block_id": "3333",
        "elements": [{
            "type": "rich_text_section",
            "elements": [{ "type": "text", "text": "Hello world" }]
        }]
    }]);
}

#[given("that edit is in the known thread")]
fn that_edit_is_in_the_known_thread(world: &mut ApiWorld) {
    let root = world
        .slack_webhook
        .known_thread
        .as_ref()
        .expect("No thread is known")
        .messages
        .first()
        .origin
        .ts
        .to_string();
    edit_of(world)["event"]["message"]["thread_ts"] = Value::String(root);
}

#[given(expr = "that edit is the one of the reply {string} in the thread {string}")]
fn that_edit_is_of_a_reply(world: &mut ApiWorld, reply_ts: String, thread_ts: String) {
    for message in ["message", "previous_message"] {
        let message = &mut edit_of(world)["event"][message];
        message["ts"] = reply_ts.clone().into();
        message["thread_ts"] = thread_ts.clone().into();
    }
}

#[given("that edit only adds a link preview to the message")]
fn that_edit_only_adds_a_link_preview(world: &mut ApiWorld) {
    let event = &mut edit_of(world)["event"];
    event["previous_message"]["text"] = event["message"]["text"].clone();
    event["previous_message"]["blocks"] = event["message"]["blocks"].clone();
    event["message"]["attachments"] = json!([{
        "id": 1,
        "title": "Example",
        "title_link": "https://example.com",
        "from_url": "https://example.com"
    }]);
}

#[given("that edit is in a thread whose timestamp is the number 42")]
fn that_edit_has_an_invalid_thread(world: &mut ApiWorld) {
    edit_of(world)["event"]["message"]["thread_ts"] = 42.into();
}

#[when("Slack delivers that event to the app")]
async fn slack_delivers_the_event(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let api_address = app.api_address.clone();
    let response = match world.slack_webhook.event.as_ref().expect("No Slack event") {
        SlackEvent::Message(event) => {
            post_signed_slack_event(&Client::new(), &api_address, &**event).await
        }
        SlackEvent::Edit(event) => {
            post_signed_slack_event(&Client::new(), &api_address, event).await
        }
    };
    world.http.record(Response::read(response).await);
}

async fn pushed_jobs(app: &TestedApp) -> Vec<JobEnvelope> {
    let payloads: Vec<Vec<u8>> = app
        .redis_storage
        .get_connection()
        .clone()
        .hvals(app.redis_storage.get_config().job_data_hash())
        .await
        .expect("Failed to get jobs");
    payloads
        .iter()
        .map(|payload| {
            let mut request: Value =
                serde_json::from_slice(payload).expect("Failed to parse job request");
            serde_json::from_value(request["args"].take()).expect("Failed to parse job payload")
        })
        .collect()
}

#[then("the event is ignored")]
async fn the_event_is_ignored(world: &mut ApiWorld) {
    let app = world.app.get().await;
    assert_eq!(pushed_jobs(app).await.len(), 0);
}

#[then("the event is queued for processing")]
async fn the_event_is_queued(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let jobs = pushed_jobs(app).await;
    assert!(
        matches!(
            jobs.as_slice(),
            [JobEnvelope {
                job: UniversalInboxJob::SlackPushEventCallback(_),
                ..
            }]
        ),
        "Expected a single Slack push event job, got {jobs:?}"
    );
}

#[then(
    expr = "the queued job is the message {string} of the thread {string}, mentioning the Slack user {string}"
)]
async fn the_queued_job_is(
    world: &mut ApiWorld,
    ts: String,
    thread_ts: String,
    slack_user_id: String,
) {
    let app = world.app.get().await;
    let jobs = pushed_jobs(app).await;
    let [
        JobEnvelope {
            job:
                UniversalInboxJob::SlackPushEventCallback(SlackPushEventCallbackJob(
                    SlackPushEventCallback {
                        event: SlackEventCallbackBody::Message(message),
                        ..
                    },
                )),
            ..
        },
    ] = jobs.as_slice()
    else {
        panic!("Expected a single Slack message job, got {jobs:?}");
    };
    assert_eq!(message.origin.ts.to_string(), ts);
    assert_eq!(
        message.origin.thread_ts.as_ref().map(|ts| ts.to_string()),
        Some(thread_ts)
    );
    assert_eq!(message.subtype, None);
    // The same function that production uses to find the mentions is the oracle here, as in the legacy test
    let references = message
        .content
        .as_ref()
        .map(universal_inbox_api::integrations::slack::find_slack_references_in_message)
        .unwrap_or_else(|| panic!("The queued message has no content: {message:?}"));
    assert!(
        references
            .users
            .contains_key(&SlackUserId(slack_user_id.clone())),
        "The queued message does not mention {slack_user_id}: {message:?}"
    );
}
