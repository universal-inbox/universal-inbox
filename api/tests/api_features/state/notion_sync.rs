use chrono::TimeDelta;
use serde_json::Value;

use universal_inbox::integration_connection::IntegrationConnection;

#[derive(Default)]
pub struct NotionSync {
    pub workspace_id: Option<String>,
    pub connection: Option<Box<IntegrationConnection>>,
    pub page: Option<WorkspacePage>,
    pub older_page: Option<TimeDelta>,
    pub comments: Vec<Value>,
    pub mounted: bool,
    pub comment_reads: Option<usize>,
    pub clock: Option<chrono::DateTime<chrono::Utc>>,
}

pub struct WorkspacePage {
    pub edited: TimeDelta,
    pub blocks: Vec<Value>,
}
