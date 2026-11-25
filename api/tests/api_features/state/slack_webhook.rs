use serde_json::Value;
use slack_morphism::prelude::SlackPushEvent;

use universal_inbox::third_party::integrations::slack::SlackThread;

pub enum SlackEvent {
    Message(Box<SlackPushEvent>),
    Edit(Value),
}

#[derive(Default)]
pub struct SlackWebhook {
    pub event: Option<SlackEvent>,
    pub known_thread: Option<Box<SlackThread>>,
}
