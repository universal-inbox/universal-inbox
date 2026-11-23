use universal_inbox::{integration_connection::IntegrationConnection, notification::Notification};

#[derive(Debug, Default)]
pub struct Integrations {
    pub github_connection: Option<Box<IntegrationConnection>>,
    pub github_notification: Option<Box<Notification>>,
}
