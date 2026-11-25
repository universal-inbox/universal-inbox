use serde_json::Value;

use universal_inbox::{integration_connection::IntegrationConnection, notification::Notification};

#[derive(Default)]
pub struct NotionWebhook {
    pub workspace_id: Option<String>,
    pub connection: Option<Box<IntegrationConnection>>,
    pub listing: Vec<Notification>,
    pub failure: Option<String>,
    pub last_job: Option<Value>,
}
