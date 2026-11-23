//! What the scenarios keep about the third-party services the account is connected to.

use universal_inbox::{integration_connection::IntegrationConnection, notification::Notification};

#[derive(Debug, Default)]
pub struct Integrations {
    pub github_connection: Option<Box<IntegrationConnection>>,
    pub github_notification: Option<Box<Notification>>,
}
