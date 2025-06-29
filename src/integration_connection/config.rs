use serde::{Deserialize, Serialize};

use crate::{
    integration_connection::{
        integrations::{
            github::GithubConfig, google_calendar::GoogleCalendarConfig,
            google_mail::GoogleMailConfig, linear::LinearConfig, slack::SlackConfig,
            todoist::TodoistConfig,
        },
        provider::IntegrationProviderKind,
    },
    notification::NotificationSourceKind,
};

// tag: New notification integration
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
#[serde(tag = "type", content = "content")]
pub enum IntegrationConnectionConfig {
    GoogleCalendar(GoogleCalendarConfig),
    GoogleDocs,
    GoogleMail(GoogleMailConfig),
    Todoist(TodoistConfig),
    Linear(LinearConfig),
    Github(GithubConfig),
    Notion,
    Slack(SlackConfig),
    TickTick,
    API,
}

impl IntegrationConnectionConfig {
    pub fn kind(&self) -> IntegrationProviderKind {
        match self {
            Self::GoogleCalendar(_) => IntegrationProviderKind::GoogleCalendar,
            Self::GoogleDocs => IntegrationProviderKind::GoogleDocs,
            Self::GoogleMail(_) => IntegrationProviderKind::GoogleMail,
            Self::Todoist(_) => IntegrationProviderKind::Todoist,
            Self::Linear(_) => IntegrationProviderKind::Linear,
            Self::Github(_) => IntegrationProviderKind::Github,
            Self::Notion => IntegrationProviderKind::Notion,
            Self::Slack(_) => IntegrationProviderKind::Slack,
            Self::TickTick => IntegrationProviderKind::TickTick,
            Self::API => IntegrationProviderKind::API,
        }
    }

    pub fn notification_source_kind(&self) -> Option<NotificationSourceKind> {
        match self {
            Self::GoogleCalendar(_) => Some(NotificationSourceKind::GoogleCalendar),
            Self::GoogleDocs => None,
            Self::GoogleMail(_) => Some(NotificationSourceKind::GoogleMail),
            Self::Todoist(_) => Some(NotificationSourceKind::Todoist),
            Self::Linear(_) => Some(NotificationSourceKind::Linear),
            Self::Github(_) => Some(NotificationSourceKind::Github),
            Self::Notion => None,
            Self::Slack(_) => Some(NotificationSourceKind::Slack),
            Self::TickTick => None,
            Self::API => Some(NotificationSourceKind::API),
        }
    }
}
