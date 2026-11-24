use http::StatusCode;

use universal_inbox::{
    integration_connection::integrations::todoist::SyncToken, task::TaskCreationResult,
    third_party::item::ThirdPartyItemCreationResult,
};
use universal_inbox_api::integrations::todoist::TodoistSyncResponse;

#[derive(Default)]
pub struct TodoistTasks {
    pub reported: Option<TodoistSyncResponse>,
    pub expected_sync_token: Option<SyncToken>,
    pub known: Vec<ThirdPartyItemCreationResult>,
    pub results: Vec<TaskCreationResult>,
    pub status: Option<StatusCode>,
    pub listed: usize,
    pub requests_noted: Option<usize>,
}
