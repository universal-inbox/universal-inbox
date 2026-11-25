use serde_json::Value;
use slack_morphism::prelude::SlackPushEvent;

use universal_inbox::{
    integration_connection::IntegrationConnection, notification::Notification,
    third_party::integrations::slack::SlackThread,
};

pub enum SlackEvent {
    Message(Box<SlackPushEvent>),
    Edit(Value),
}

#[derive(Default)]
pub struct SlackWebhook {
    pub event: Option<SlackEvent>,
    pub known_thread: Option<Box<SlackThread>>,
    pub other_connection: Option<Box<IntegrationConnection>>,
    pub runs: Vec<Vec<&'static str>>,
    pub failure: Option<String>,
    pub listing: Vec<Notification>,
    pub unresolved: Vec<bool>,
}
