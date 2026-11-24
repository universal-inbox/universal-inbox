use universal_inbox::{
    integration_connection::IntegrationConnection,
    notification::Notification,
    task::{ProjectSummary, Task},
};

#[derive(Debug, Default)]
pub struct Integrations {
    pub github_connection: Option<Box<IntegrationConnection>>,
    pub github_notification: Option<Box<Notification>>,
    pub todoist_connection: Option<Box<IntegrationConnection>>,
    pub linear_connection: Option<Box<IntegrationConnection>>,
    pub linear_project: Option<ProjectSummary>,
    pub linear_task: Option<Task>,
    pub other_github_connection: Option<Box<IntegrationConnection>>,
    pub google_connection: Option<Box<IntegrationConnection>>,
    pub google_mail_connection: Option<Box<IntegrationConnection>>,
    pub google_calendar_connection: Option<Box<IntegrationConnection>>,
    pub other_linear_connection: Option<Box<IntegrationConnection>>,
    pub google_drive_connection: Option<Box<IntegrationConnection>>,
    pub ticktick_connection: Option<Box<IntegrationConnection>>,
    pub slack_connection: Option<Box<IntegrationConnection>>,
    pub oauth_connection: Option<Box<IntegrationConnection>>,
}
