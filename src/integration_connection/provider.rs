use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{
            github::GithubConfig,
            google_mail::{GoogleMailConfig, GoogleMailContext},
            linear::LinearConfig,
            slack::{SlackConfig, SlackSyncTaskConfig, SlackSyncType},
            todoist::{TodoistConfig, TodoistContext},
        },
    },
    task::{
        integrations::todoist::TODOIST_INBOX_PROJECT, ProjectSummary, TaskCreation, TaskPriority,
    },
};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
#[serde(tag = "type", content = "content")]
pub enum IntegrationProvider {
    Github {
        config: GithubConfig,
    },
    Linear {
        config: LinearConfig,
    },
    GoogleMail {
        context: Option<GoogleMailContext>,
        config: GoogleMailConfig,
    },
    Notion,
    GoogleDocs,
    Slack {
        config: SlackConfig,
    },
    Todoist {
        context: Option<TodoistContext>,
        config: TodoistConfig,
    },
    TickTick,
}

impl IntegrationProvider {
    pub fn new(
        config: IntegrationConnectionConfig,
        context: Option<IntegrationConnectionContext>,
    ) -> Result<Self> {
        match config {
            IntegrationConnectionConfig::Github(config) => Ok(Self::Github { config }),
            IntegrationConnectionConfig::Linear(config) => Ok(Self::Linear { config }),
            IntegrationConnectionConfig::GoogleMail(config) => Ok(Self::GoogleMail {
                context: context
                    .map(|c| {
                        if let IntegrationConnectionContext::GoogleMail(c) = c {
                            Ok(c)
                        } else {
                            Err(anyhow!("Unexpect context for Google Mail provider: {c:?}"))
                        }
                    })
                    .transpose()?,
                config,
            }),
            IntegrationConnectionConfig::Notion => Ok(Self::Notion),
            IntegrationConnectionConfig::GoogleDocs => Ok(Self::GoogleDocs),
            IntegrationConnectionConfig::Slack(config) => Ok(Self::Slack { config }),
            IntegrationConnectionConfig::Todoist(config) => Ok(Self::Todoist {
                context: context
                    .map(|c| {
                        if let IntegrationConnectionContext::Todoist(c) = c {
                            Ok(c)
                        } else {
                            Err(anyhow!("Unexpect context for Todoist provider: {c:?}"))
                        }
                    })
                    .transpose()?,
                config,
            }),
            IntegrationConnectionConfig::TickTick => Ok(Self::TickTick),
        }
    }

    pub fn is_task_service(&self) -> bool {
        self.kind().is_task_service()
    }

    pub fn is_notification_service(&self) -> bool {
        self.kind().is_notification_service()
    }

    pub fn kind(&self) -> IntegrationProviderKind {
        match self {
            IntegrationProvider::Github { .. } => IntegrationProviderKind::Github,
            IntegrationProvider::Linear { .. } => IntegrationProviderKind::Linear,
            IntegrationProvider::GoogleMail { .. } => IntegrationProviderKind::GoogleMail,
            IntegrationProvider::Notion => IntegrationProviderKind::Notion,
            IntegrationProvider::GoogleDocs => IntegrationProviderKind::GoogleDocs,
            IntegrationProvider::Slack { .. } => IntegrationProviderKind::Slack,
            IntegrationProvider::Todoist { .. } => IntegrationProviderKind::Todoist,
            IntegrationProvider::TickTick => IntegrationProviderKind::TickTick,
        }
    }

    pub fn config(&self) -> IntegrationConnectionConfig {
        match self {
            IntegrationProvider::Github { config } => {
                IntegrationConnectionConfig::Github(config.clone())
            }
            IntegrationProvider::Linear { config } => {
                IntegrationConnectionConfig::Linear(config.clone())
            }
            IntegrationProvider::GoogleMail { config, .. } => {
                IntegrationConnectionConfig::GoogleMail(config.clone())
            }
            IntegrationProvider::Todoist { config, .. } => {
                IntegrationConnectionConfig::Todoist(config.clone())
            }
            IntegrationProvider::Notion => IntegrationConnectionConfig::Notion,
            IntegrationProvider::GoogleDocs => IntegrationConnectionConfig::GoogleDocs,
            IntegrationProvider::Slack { config } => {
                IntegrationConnectionConfig::Slack(config.clone())
            }
            IntegrationProvider::TickTick => IntegrationConnectionConfig::TickTick,
        }
    }

    pub fn is_sync_notifications_enabled(&self) -> bool {
        match self {
            IntegrationProvider::Github { config } => config.sync_notifications_enabled,
            IntegrationProvider::Linear { config } => config.sync_notifications_enabled,
            IntegrationProvider::GoogleMail { config, .. } => config.sync_notifications_enabled,
            IntegrationProvider::Slack { config } => {
                config.sync_enabled && config.sync_type == SlackSyncType::AsNotifications
            }
            _ => false,
        }
    }

    pub fn is_sync_tasks_enabled(&self) -> bool {
        match self {
            IntegrationProvider::Todoist { config, .. } => config.sync_tasks_enabled,
            IntegrationProvider::Linear { config } => config.sync_task_config.enabled,
            _ => false,
        }
    }

    pub fn should_create_notification_from_inbox_task(&self) -> bool {
        match self {
            IntegrationProvider::Todoist { config, .. } => {
                config.create_notification_from_inbox_task
            }
            _ => false,
        }
    }

    pub fn get_task_creation_default_values(&self) -> Option<TaskCreation> {
        match self {
            IntegrationProvider::Slack {
                config:
                    SlackConfig {
                        sync_type:
                            SlackSyncType::AsTasks(SlackSyncTaskConfig {
                                target_project,
                                default_due_at,
                                default_priority,
                            }),
                        ..
                    },
            } => Some(TaskCreation {
                title: "Unused".to_string(),
                body: None,
                project: target_project.clone().unwrap_or_else(|| ProjectSummary {
                    source_id: "Unused".to_string(),
                    name: TODOIST_INBOX_PROJECT.to_string(),
                }),
                due_at: default_due_at.as_ref().map(|due_at| due_at.clone().into()),
                priority: *default_priority,
            }),
            IntegrationProvider::Linear { config } => Some(TaskCreation {
                title: "Unused".to_string(),
                body: None,
                project: config
                    .sync_task_config
                    .target_project
                    .clone()
                    .unwrap_or_else(|| ProjectSummary {
                        source_id: "Unused".to_string(),
                        name: TODOIST_INBOX_PROJECT.to_string(),
                    }),
                due_at: None,
                priority: TaskPriority::default(),
            }),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
#[serde(tag = "type", content = "content")]
pub enum IntegrationConnectionContext {
    Todoist(TodoistContext),
    GoogleMail(GoogleMailContext),
}

pub trait IntegrationProviderSource {
    fn get_integration_provider_kind(&self) -> IntegrationProviderKind;
}

macro_attr! {
    // tag: New notification integration
    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy, Eq, EnumFromStr!, EnumDisplay!, Hash)]
    pub enum IntegrationProviderKind {
        Github,
        Linear,
        GoogleMail,
        Notion,
        GoogleDocs,
        Slack,
        Todoist,
        TickTick
    }
}

impl IntegrationProviderKind {
    pub fn is_task_service(&self) -> bool {
        *self == IntegrationProviderKind::Todoist || *self == IntegrationProviderKind::TickTick
    }

    // tag: New notification integration
    pub fn is_notification_service(&self) -> bool {
        *self == IntegrationProviderKind::Github
            || *self == IntegrationProviderKind::Linear
            || *self == IntegrationProviderKind::GoogleMail
            || *self == IntegrationProviderKind::Notion
            || *self == IntegrationProviderKind::GoogleDocs
            || *self == IntegrationProviderKind::Slack
    }

    pub fn default_integration_connection_config(&self) -> IntegrationConnectionConfig {
        match self {
            IntegrationProviderKind::Github => {
                IntegrationConnectionConfig::Github(GithubConfig::default())
            }
            IntegrationProviderKind::Linear => {
                IntegrationConnectionConfig::Linear(Default::default())
            }
            IntegrationProviderKind::GoogleMail => {
                IntegrationConnectionConfig::GoogleMail(Default::default())
            }
            IntegrationProviderKind::Notion => IntegrationConnectionConfig::Notion,
            IntegrationProviderKind::GoogleDocs => IntegrationConnectionConfig::GoogleDocs,
            IntegrationProviderKind::Slack => {
                IntegrationConnectionConfig::Slack(Default::default())
            }
            IntegrationProviderKind::Todoist => {
                IntegrationConnectionConfig::Todoist(Default::default())
            }
            IntegrationProviderKind::TickTick => IntegrationConnectionConfig::TickTick,
        }
    }
}
