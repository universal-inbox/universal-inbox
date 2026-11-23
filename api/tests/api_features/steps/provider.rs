use std::str::FromStr;

use universal_inbox::integration_connection::provider::IntegrationProviderKind;

#[derive(Clone, Copy, Debug)]
pub struct Provider(pub IntegrationProviderKind);

impl FromStr for Provider {
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
