use universal_inbox::integration_connection::IntegrationConnection;

#[derive(Default)]
pub struct NotionWebhook {
    pub workspace_id: Option<String>,
    pub connection: Option<Box<IntegrationConnection>>,
}
