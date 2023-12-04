use serde::{Deserialize, Serialize};

use crate::integration_connection::{
    integrations::{
        github::GithubConfig, google_mail::GoogleMailConfig, linear::LinearConfig,
        todoist::TodoistConfig,
    },
    provider::IntegrationProviderKind,
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
}
