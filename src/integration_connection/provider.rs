use anyhow::{Result, anyhow};
use clap::ValueEnum;
use schemars::JsonSchema;
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
                SlackConfig, SlackContext, SlackReactionConfig, SlackSyncTaskConfig, SlackSyncType,
            },
            ticktick::{TickTickConfig, TickTickContext},
            todoist::{TodoistConfig, TodoistContext},
        },
    },
    task::{DueDate, TaskCreationConfig, TaskPriority},
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
            IntegrationProvider::Slack { config, .. } => config.message_config.sync_enabled,
            _ => false,
        }
    }

    /// Whether the user has switched this integration's notifications off.
    ///
    /// Scoped to the providers whose configuration actually carries a
    /// notification sync flag. `is_sync_notifications_enabled()` answers `false`
    /// for every other provider, so reusing it here would set aside Todoist,
    /// TickTick, Google Calendar, Notion and API notifications on any
    /// configuration save, with no notifications sync of their own to ever
    /// restore them.
    pub fn are_notifications_muted(&self) -> bool {
        match self {
            IntegrationProvider::Github { .. }
            | IntegrationProvider::Linear { .. }
            | IntegrationProvider::GoogleDrive { .. }
            | IntegrationProvider::GoogleMail { .. }
            | IntegrationProvider::Slack { .. } => !self.is_sync_notifications_enabled(),
            // Todoist and TickTick notifications are a byproduct of their task
            // sync, and the already collected ones stay in the inbox. Google
            // Calendar, Notion and API have no notification mute toggle.
            _ => false,
        }
    }

    /// Whether a synchronization exists that could ever bring this
    /// integration's notifications back into the inbox.
    ///
    /// Notifications are only ever set aside when something can restore them.
    /// `API` notifications are pushed in by an external client rather than
    /// collected by a synchronization — `NotificationSyncSourceKind` has no
    /// `API` variant — so setting them aside would hide them for good.
    pub fn can_restore_set_aside_notifications(&self) -> bool {
        !matches!(self, IntegrationProvider::API)
    }

    /// Whether this integration's notifications are reconciled by its *task*
    /// synchronization rather than by a notifications synchronization of its
    /// own.
    ///
    /// Todoist and TickTick notifications are a byproduct of their task sync,
    /// which never reaches `complete_notifications_sync_status`, so that
    /// completion is the only one that could bring back whatever disconnecting
    /// them set aside. Linear also synchronizes tasks, but its notifications are
    /// reconciled by its own notifications sync — revealing them on a task sync
    /// would show an inbox no stale pass had checked.
    pub fn are_notifications_reconciled_by_tasks_sync(&self) -> bool {
        matches!(
            self,
            IntegrationProvider::Todoist { .. } | IntegrationProvider::TickTick { .. }
        )
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

    pub fn is_auto_delete_notifications_on_task_sync_enabled(&self) -> bool {
        match self {
            IntegrationProvider::Linear { config } => {
                config.sync_task_config.enabled && config.sync_task_config.auto_delete_notifications
            }
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
        let (
            target_project,
            default_due_at,
            default_priority,
            task_manager_provider_kind,
            default_time_config,
        ) = match self {
            IntegrationProvider::Slack { config, .. } => {
                match third_party_item.get_third_party_item_source_kind() {
                    ThirdPartyItemSourceKind::SlackReaction => {
                        let SlackConfig {
                            reaction_config:
                                SlackReactionConfig {
                                    sync_type:
                                        SlackSyncType::AsTasks(SlackSyncTaskConfig {
                                            target_project,
                                            default_due_at,
                                            default_priority,
                                            task_manager_provider_kind,
                                            default_time_config,
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
                            task_manager_provider_kind.as_ref(),
                            default_time_config.as_ref(),
                        )
                    }
                    _ => return None,
                }
            }
            IntegrationProvider::Linear { config } => (
                config.sync_task_config.target_project.as_ref(),
                config.sync_task_config.default_due_at.as_ref(),
                &TaskPriority::default(),
                config.sync_task_config.task_manager_provider_kind.as_ref(),
                config.sync_task_config.default_time_config.as_ref(),
            ),
            _ => return None,
        };

        let due_at = default_due_at
            .map(|due_at| due_at.clone().into())
            .map(|due: DueDate| match default_time_config {
                Some(time_config) => due.with_time_config(time_config),
                None => due,
            });

        Some(TaskCreationConfig {
            project_name: target_project.map(|project| project.name.clone()),
            due_at,
            priority: *default_priority,
            task_manager_provider_kind: task_manager_provider_kind.copied(),
            time_config: default_time_config.cloned(),
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
    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy, Eq, EnumFromStr!, EnumDisplay!, Hash, ValueEnum, JsonSchema)]
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
