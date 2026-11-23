use std::collections::HashMap;

use rmcp::{model::Implementation, transport::streamable_http_server::session::SessionState};
use slack_morphism::prelude::{SlackEmojiName, SlackEmojiRef};

#[derive(Default)]
pub struct McpSessions {
    pub ttl_seconds: u64,
    pub id: Option<String>,
    pub stored_client: Option<Implementation>,
    pub loaded: Option<Option<SessionState>>,
}

#[derive(Default)]
pub struct SlackEmojis {
    pub tokens: HashMap<String, String>,
    pub listed: Vec<(String, HashMap<SlackEmojiName, SlackEmojiRef>)>,
}
