use reqwest::{Client, StatusCode};
use serde_json::Value;

use universal_inbox::auth::auth_token::AuthenticationToken;

#[derive(Default)]
pub struct McpProtocol {
    pub client: Option<Client>,
    pub session_id: Option<String>,
    pub other_api_token: Option<AuthenticationToken>,
    pub status: Option<StatusCode>,
    pub body: Option<Value>,
}
