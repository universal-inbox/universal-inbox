use std::str::FromStr;

use cucumber::Parameter;
use universal_inbox::integration_connection::provider::IntegrationProviderKind;

use crate::helpers::TestedApp;

#[derive(Clone, Copy, Debug)]
pub struct ProviderName(pub IntegrationProviderKind);

impl Parameter for ProviderName {
    const REGEX: &'static str =
        "GitHub|Linear|Slack|Todoist|Google Mail|Google Calendar|Google Drive|Notion|TickTick";
    const NAME: &'static str = "provider";
}

impl FromStr for ProviderName {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Ok(Self(match name {
            "GitHub" => IntegrationProviderKind::Github,
            "Linear" => IntegrationProviderKind::Linear,
            "Slack" => IntegrationProviderKind::Slack,
            "Todoist" => IntegrationProviderKind::Todoist,
            "Google Mail" => IntegrationProviderKind::GoogleMail,
            "Google Calendar" => IntegrationProviderKind::GoogleCalendar,
            "Google Drive" => IntegrationProviderKind::GoogleDrive,
            "Notion" => IntegrationProviderKind::Notion,
            "TickTick" => IntegrationProviderKind::TickTick,
            other => return Err(format!("Unknown provider {other:?}")),
        }))
    }
}

impl ProviderName {
    pub fn mock_server<'a>(&self, app: &'a TestedApp) -> &'a wiremock::MockServer {
        match self.0 {
            IntegrationProviderKind::Github => &app.github_mock_server,
            IntegrationProviderKind::Linear => &app.linear_mock_server,
            IntegrationProviderKind::Slack => &app.slack_mock_server,
            IntegrationProviderKind::Todoist => &app.todoist_mock_server,
            IntegrationProviderKind::GoogleMail => &app.google_mail_mock_server,
            IntegrationProviderKind::GoogleCalendar => &app.google_calendar_mock_server,
            IntegrationProviderKind::GoogleDrive => &app.google_drive_mock_server,
            IntegrationProviderKind::Notion => &app.notion_mock_server,
            IntegrationProviderKind::TickTick => &app.ticktick_mock_server,
            IntegrationProviderKind::API => panic!("The API has no mock server"),
        }
    }
}
