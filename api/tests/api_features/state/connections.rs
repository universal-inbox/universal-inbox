use universal_inbox::integration_connection::IntegrationConnection;
use universal_inbox_api::universal_inbox::UniversalInboxError;

#[derive(Default)]
pub struct Connections {
    pub created: Vec<IntegrationConnection>,
    pub listed: Vec<IntegrationConnection>,
    pub disconnected: Option<IntegrationConnection>,
    pub google_calendar: Option<IntegrationConnection>,
    pub token_lookup: Option<Result<String, UniversalInboxError>>,
}
