use anyhow::{Result, anyhow};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{
            github::GithubConfig,
            google_calendar::GoogleCalendarConfig,
            google_drive::{GoogleDriveConfig, GoogleDriveContext},
            google_mail::{GoogleMailConfig, GoogleMailContext},
            linear::LinearConfig,
            slack::{
                SlackConfig, SlackContext, SlackReactionConfig, SlackStarConfig,
                SlackSyncTaskConfig, SlackSyncType,
            },
            ticktick::{TickTickConfig, TickTickContext},
            todoist::{TodoistConfig, TodoistContext},
        },
    },
    task::{TaskCreationConfig, TaskPriority},
    third_party::item::{ThirdPartyItem, ThirdPartyItemSource, ThirdPartyItemSourceKind},
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
    GoogleCalendar {
        config: GoogleCalendarConfig,
    },
    GoogleDrive {
        context: Option<GoogleDriveContext>,
        config: GoogleDriveConfig,
    },
    GoogleMail {
        context: Option<GoogleMailContext>,
        config: GoogleMailConfig,
    },
    Notion,
    Slack {
        context: Option<SlackContext>,
        config: SlackConfig,
    },
    Todoist {
        context: Option<TodoistContext>,
        config: TodoistConfig,
    },
    TickTick {
        context: Option<TickTickContext>,
        config: TickTickConfig,
    },
    API,
}

impl IntegrationProvider {
    pub fn new(
        config: IntegrationConnectionConfig,
        context: Option<IntegrationConnectionContext>,
    ) -> Result<Self> {
        match config {
            IntegrationConnectionConfig::Github(config) => Ok(Self::Github { config }),
            IntegrationConnectionConfig::Linear(config) => Ok(Self::Linear { config }),
            IntegrationConnectionConfig::GoogleCalendar(config) => {
                Ok(Self::GoogleCalendar { config })
            }
            IntegrationConnectionConfig::GoogleDrive(config) => Ok(Self::GoogleDrive {
                context: context
                    .map(|c| {
                        if let IntegrationConnectionContext::GoogleDrive(c) = c {
                            Ok(c)
                        } else {
                            Err(anyhow!("Unexpect context for Google Drive provider: {c:?}"))
                        }
                    })
                    .transpose()?,
                config,
            }),
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
            IntegrationConnectionConfig::Slack(config) => Ok(Self::Slack {
                context: context
                    .map(|c| {
                        if let IntegrationConnectionContext::Slack(c) = c {
                            Ok(c)
                        } else {
                            Err(anyhow!("Unexpect context for Slack provider: {c:?}"))
                        }
                    })
                    .transpose()?,
                config,
            }),
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
            IntegrationConnectionConfig::TickTick(config) => Ok(Self::TickTick {
                context: context
                    .map(|c| {
                        if let IntegrationConnectionContext::TickTick(c) = c {
                            Ok(c)
                        } else {
                            Err(anyhow!("Unexpect context for TickTick provider: {c:?}"))
                        }
                    })
                    .transpose()?,
                config,
            }),
            IntegrationConnectionConfig::API => Ok(Self::API),
        }
    }

    pub fn context_is_empty(&self) -> bool {
        match self {
            IntegrationProvider::Github { .. } => false,
            IntegrationProvider::Linear { .. } => false,
            IntegrationProvider::GoogleCalendar { .. } => false,
            IntegrationProvider::GoogleDrive { context, .. } => context.is_none(),
            IntegrationProvider::GoogleMail { context, .. } => context.is_none(),
            IntegrationProvider::Notion => false,
            IntegrationProvider::Slack { context, .. } => context.is_none(),
            IntegrationProvider::Todoist { context, .. } => context.is_none(),
            IntegrationProvider::TickTick { context, .. } => context.is_none(),
            IntegrationProvider::API => false,
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
            IntegrationProvider::GoogleCalendar { .. } => IntegrationProviderKind::GoogleCalendar,
            IntegrationProvider::GoogleDrive { .. } => IntegrationProviderKind::GoogleDrive,
            IntegrationProvider::GoogleMail { .. } => IntegrationProviderKind::GoogleMail,
            IntegrationProvider::Notion => IntegrationProviderKind::Notion,
            IntegrationProvider::Slack { .. } => IntegrationProviderKind::Slack,
            IntegrationProvider::Todoist { .. } => IntegrationProviderKind::Todoist,
            IntegrationProvider::TickTick { .. } => IntegrationProviderKind::TickTick,
            IntegrationProvider::API => IntegrationProviderKind::API,
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
            IntegrationProvider::GoogleCalendar { config } => {
                IntegrationConnectionConfig::GoogleCalendar(config.clone())
            }
            IntegrationProvider::GoogleDrive { config, .. } => {
                IntegrationConnectionConfig::GoogleDrive(config.clone())
            }
            IntegrationProvider::GoogleMail { config, .. } => {
                IntegrationConnectionConfig::GoogleMail(config.clone())
            }
            IntegrationProvider::Todoist { config, .. } => {
                IntegrationConnectionConfig::Todoist(config.clone())
            }
            IntegrationProvider::Notion => IntegrationConnectionConfig::Notion,
            IntegrationProvider::Slack { config, .. } => {
                IntegrationConnectionConfig::Slack(config.clone())
            }
            IntegrationProvider::TickTick { config, .. } => {
                IntegrationConnectionConfig::TickTick(config.clone())
            }
            IntegrationProvider::API => IntegrationConnectionConfig::API,
        }
    }

    pub fn is_sync_notifications_enabled(&self) -> bool {
        match self {
            IntegrationProvider::Github { config } => config.sync_notifications_enabled,
            IntegrationProvider::Linear { config } => config.sync_notifications_enabled,
            IntegrationProvider::GoogleDrive { config, .. } => config.sync_notifications_enabled,
            IntegrationProvider::GoogleMail { config, .. } => config.sync_notifications_enabled,
            IntegrationProvider::Slack { .. } => false, // Slack notifications are not synced but received via the webhook
            _ => false,
        }
    }

    pub fn is_sync_tasks_enabled(&self) -> bool {
        match self {
            IntegrationProvider::Todoist { config, .. } => config.sync_tasks_enabled,
            IntegrationProvider::TickTick { config, .. } => config.sync_tasks_enabled,
            IntegrationProvider::Linear { config } => config.sync_task_config.enabled,
            IntegrationProvider::Slack { .. } => false, // Slack tasks are not synced but received via the webhook
            _ => false,
        }
    }

    pub fn should_create_notification_from_inbox_task(&self) -> bool {
        match self {
            IntegrationProvider::Todoist { config, .. } => {
                config.create_notification_from_inbox_task
            }
            IntegrationProvider::TickTick { config, .. } => {
                config.create_notification_from_inbox_task
            }
            _ => false,
        }
    }

    pub fn get_task_creation_default_values(
        &self,
        third_party_item: &ThirdPartyItem,
    ) -> Option<TaskCreationConfig> {
        let (target_project, default_due_at, default_priority) = match self {
            IntegrationProvider::Slack { config, .. } => {
                match third_party_item.get_third_party_item_source_kind() {
                    ThirdPartyItemSourceKind::SlackStar => {
                        let SlackConfig {
                            star_config:
                                SlackStarConfig {
                                    sync_type:
                                        SlackSyncType::AsTasks(SlackSyncTaskConfig {
                                            target_project,
                                            default_due_at,
                                            default_priority,
                                        }),
                                    ..
                                },
                            ..
                        } = config
                        else {
                            return None;
                        };

                        (
                            target_project.as_ref(),
                            default_due_at.as_ref(),
                            default_priority,
                        )
                    }
                    ThirdPartyItemSourceKind::SlackReaction => {
                        let SlackConfig {
                            reaction_config:
                                SlackReactionConfig {
                                    sync_type:
                                        SlackSyncType::AsTasks(SlackSyncTaskConfig {
                                            target_project,
                                            default_due_at,
                                            default_priority,
                                        }),
                                    ..
                                },
                            ..
                        } = config
                        else {
                            return None;
                        };

                        (
                            target_project.as_ref(),
                            default_due_at.as_ref(),
                            default_priority,
                        )
                    }
                    _ => return None,
                }
            }
            IntegrationProvider::Linear { config } => (
                config.sync_task_config.target_project.as_ref(),
                config.sync_task_config.default_due_at.as_ref(),
                &TaskPriority::default(),
            ),
            _ => return None,
        };

        Some(TaskCreationConfig {
            project_name: target_project.map(|project| project.name.clone()),
            due_at: default_due_at.map(|due_at| due_at.clone().into()),
            priority: *default_priority,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
#[serde(tag = "type", content = "content")]
pub enum IntegrationConnectionContext {
    Todoist(TodoistContext),
    TickTick(TickTickContext),
    GoogleDrive(GoogleDriveContext),
    GoogleMail(GoogleMailContext),
    Slack(SlackContext),
}

pub trait IntegrationProviderSource {
    fn get_integration_provider_kind(&self) -> IntegrationProviderKind;
}

macro_attr! {
    // tag: New notification integration
    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy, Eq, EnumFromStr!, EnumDisplay!, Hash, ValueEnum)]
    pub enum IntegrationProviderKind {
        Github,
        Linear,
        GoogleCalendar,
        GoogleDrive,
        GoogleMail,
        Notion,
        Slack,
        Todoist,
        TickTick,
        API,
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
            || *self == IntegrationProviderKind::GoogleDrive
            || *self == IntegrationProviderKind::GoogleMail
            || *self == IntegrationProviderKind::Notion
            || *self == IntegrationProviderKind::Slack
            || *self == IntegrationProviderKind::API
    }

    pub fn default_integration_connection_config(&self) -> IntegrationConnectionConfig {
        match self {
            IntegrationProviderKind::Github => {
                IntegrationConnectionConfig::Github(GithubConfig::default())
            }
            IntegrationProviderKind::Linear => {
                IntegrationConnectionConfig::Linear(Default::default())
            }
            IntegrationProviderKind::GoogleCalendar => {
                IntegrationConnectionConfig::GoogleCalendar(Default::default())
            }
            IntegrationProviderKind::GoogleDrive => {
                IntegrationConnectionConfig::GoogleDrive(Default::default())
            }
            IntegrationProviderKind::GoogleMail => {
                IntegrationConnectionConfig::GoogleMail(Default::default())
            }
            IntegrationProviderKind::Notion => IntegrationConnectionConfig::Notion,
            IntegrationProviderKind::Slack => {
                IntegrationConnectionConfig::Slack(Default::default())
            }
            IntegrationProviderKind::Todoist => {
                IntegrationConnectionConfig::Todoist(Default::default())
            }
            IntegrationProviderKind::TickTick => {
                IntegrationConnectionConfig::TickTick(Default::default())
            }
            IntegrationProviderKind::API => IntegrationConnectionConfig::API,
        }
    }
}
