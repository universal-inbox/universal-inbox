use reqwest::{Client, StatusCode};
use serde_json::Value;
use universal_inbox::{
    auth::auth_token::AuthenticationToken, notification::Notification,
    third_party::integrations::todoist::TodoistItem,
};

#[derive(Default)]
pub struct McpProtocol {
    pub client: Option<Client>,
    pub session_id: Option<String>,
    pub other_api_token: Option<AuthenticationToken>,
    pub status: Option<StatusCode>,
    pub body: Option<Value>,
    pub notifications: Vec<(String, Notification)>,
    pub todoist_item: Option<Box<TodoistItem>>,
    pub task_id: Option<String>,
    pub expected_github_calls: Vec<(&'static str, String)>,
}
