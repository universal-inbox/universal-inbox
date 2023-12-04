use serde::{Deserialize, Serialize};

use crate::{
    integration_connection::{
        integrations::{
            github::GithubConfig, google_mail::GoogleMailConfig, linear::LinearConfig,
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
    GoogleMail(GoogleMailConfig),
    Todoist(TodoistConfig),
    Linear(LinearConfig),
    Github(GithubConfig),
    Notion,
    GoogleDocs,
    Slack,
    TickTick,
}

impl IntegrationConnectionConfig {
    pub fn kind(&self) -> IntegrationProviderKind {
        match self {
            Self::GoogleMail(_) => IntegrationProviderKind::GoogleMail,
            Self::Todoist(_) => IntegrationProviderKind::Todoist,
            Self::Linear(_) => IntegrationProviderKind::Linear,
            Self::Github(_) => IntegrationProviderKind::Github,
            Self::Notion => IntegrationProviderKind::Notion,
            Self::GoogleDocs => IntegrationProviderKind::GoogleDocs,
            Self::Slack => IntegrationProviderKind::Slack,
            Self::TickTick => IntegrationProviderKind::TickTick,
        }
    }

    pub fn notification_source_kind(&self) -> Option<NotificationSourceKind> {
        match self {
            Self::GoogleMail(_) => Some(NotificationSourceKind::GoogleMail),
            Self::Todoist(_) => Some(NotificationSourceKind::Todoist),
            Self::Linear(_) => Some(NotificationSourceKind::Linear),
            Self::Github(_) => Some(NotificationSourceKind::Github),
            Self::Notion => None,
            Self::GoogleDocs => None,
            Self::Slack => None,
            Self::TickTick => None,
        }
    }
}
