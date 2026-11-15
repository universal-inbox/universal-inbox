use serde::{Deserialize, Serialize};

use crate::{
    integration_connection::{
        integrations::{
            github::GithubConfig,
            google_calendar::GoogleCalendarConfig,
            google_drive::GoogleDriveConfig,
            google_mail::GoogleMailConfig,
            linear::LinearConfig,
            notion::NotionConfig,
            slack::{SlackConfig, SlackReactionConfig, SlackSyncType},
            ticktick::TickTickConfig,
            todoist::TodoistConfig,
        },
        provider::IntegrationProviderKind,
    },
    notification::NotificationSourceKind,
    task::{DefaultProject, ProjectReference},
};

// tag: New notification integration
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
#[serde(tag = "type", content = "content")]
pub enum IntegrationConnectionConfig {
    GoogleCalendar(GoogleCalendarConfig),
    GoogleDrive(GoogleDriveConfig),
    GoogleMail(GoogleMailConfig),
    Todoist(TodoistConfig),
    Linear(LinearConfig),
    Github(GithubConfig),
    Notion(NotionConfig),
    Slack(SlackConfig),
    TickTick(TickTickConfig),
    API,
}

/// The stored default project of a config, and the task manager it is meant for when the config
/// says: Todoist and TickTick configs are their own task manager, a Linear or Slack config names
/// one or leaves it to the user preference (`None`).
pub struct DefaultProjectSlot<'a> {
    pub project: &'a mut Option<ProjectReference>,
    pub task_manager: Option<IntegrationProviderKind>,
}

impl IntegrationConnectionConfig {
    /// The default project slot of the config: Todoist and TickTick `default_project`, Linear
    /// `target_project`, Slack `target_project` when reactions sync as tasks. `None` for a
    /// config without one.
    pub fn default_project_slot(&mut self) -> Option<DefaultProjectSlot<'_>> {
        match self {
            Self::Todoist(config) => Some(DefaultProjectSlot {
                project: &mut config.default_project,
                task_manager: Some(IntegrationProviderKind::Todoist),
            }),
            Self::TickTick(config) => Some(DefaultProjectSlot {
                project: &mut config.default_project,
                task_manager: Some(IntegrationProviderKind::TickTick),
            }),
            Self::Linear(config) => Some(DefaultProjectSlot {
                task_manager: config.sync_task_config.task_manager_provider_kind,
                project: &mut config.sync_task_config.target_project,
            }),
            Self::Slack(SlackConfig {
                reaction_config:
                    SlackReactionConfig {
                        sync_type: SlackSyncType::AsTasks(task_config),
                        ..
                    },
                ..
            }) => Some(DefaultProjectSlot {
                task_manager: task_config.task_manager_provider_kind,
                project: &mut task_config.target_project,
            }),
            _ => None,
        }
    }

    /// The default project of the config, read for the task manager it is meant for, and that
    /// task manager. `preferred_task_manager`: the user's, for a Linear or Slack config that
    /// does not name one. `None` without a default project.
    pub fn default_project(
        &self,
        preferred_task_manager: IntegrationProviderKind,
    ) -> Option<(DefaultProject, IntegrationProviderKind)> {
        let (Some(reference), task_manager) = self.stored_default_project()? else {
            return None;
        };
        let task_manager = task_manager.unwrap_or(preferred_task_manager);
        Some((DefaultProject::new(reference, task_manager), task_manager))
    }

    /// The task manager the default project of the config is meant for (see
    /// [`Self::default_project`]), whether a default is set or not
    pub fn default_project_task_manager(
        &self,
        preferred_task_manager: IntegrationProviderKind,
    ) -> Option<IntegrationProviderKind> {
        self.stored_default_project()
            .map(|(_, task_manager)| task_manager.unwrap_or(preferred_task_manager))
    }

    /// The stored default project of the config and the task manager the config names, as
    /// stored: read it through [`Self::default_project`]
    fn stored_default_project(
        &self,
    ) -> Option<(Option<&ProjectReference>, Option<IntegrationProviderKind>)> {
        match self {
            Self::Todoist(config) => Some((
                config.default_project.as_ref(),
                Some(IntegrationProviderKind::Todoist),
            )),
            Self::TickTick(config) => Some((
                config.default_project.as_ref(),
                Some(IntegrationProviderKind::TickTick),
            )),
            Self::Linear(config) => Some((
                config.sync_task_config.target_project.as_ref(),
                config.sync_task_config.task_manager_provider_kind,
            )),
            Self::Slack(SlackConfig {
                reaction_config:
                    SlackReactionConfig {
                        sync_type: SlackSyncType::AsTasks(task_config),
                        ..
                    },
                ..
            }) => Some((
                task_config.target_project.as_ref(),
                task_config.task_manager_provider_kind,
            )),
            _ => None,
        }
    }

    pub fn kind(&self) -> IntegrationProviderKind {
        match self {
            Self::GoogleCalendar(_) => IntegrationProviderKind::GoogleCalendar,
            Self::GoogleDrive(_) => IntegrationProviderKind::GoogleDrive,
            Self::GoogleMail(_) => IntegrationProviderKind::GoogleMail,
            Self::Todoist(_) => IntegrationProviderKind::Todoist,
            Self::Linear(_) => IntegrationProviderKind::Linear,
            Self::Github(_) => IntegrationProviderKind::Github,
            Self::Notion(_) => IntegrationProviderKind::Notion,
            Self::Slack(_) => IntegrationProviderKind::Slack,
            Self::TickTick(_) => IntegrationProviderKind::TickTick,
            Self::API => IntegrationProviderKind::API,
        }
    }

    pub fn notification_source_kind(&self) -> Option<NotificationSourceKind> {
        match self {
            Self::GoogleCalendar(_) => Some(NotificationSourceKind::GoogleCalendar),
            Self::GoogleDrive(_) => Some(NotificationSourceKind::GoogleDrive),
            Self::GoogleMail(_) => Some(NotificationSourceKind::GoogleMail),
            Self::Todoist(_) => Some(NotificationSourceKind::Todoist),
            Self::Linear(_) => Some(NotificationSourceKind::Linear),
            Self::Github(_) => Some(NotificationSourceKind::Github),
            Self::Notion(_) => Some(NotificationSourceKind::Notion),
            Self::Slack(_) => Some(NotificationSourceKind::Slack),
            Self::TickTick(_) => Some(NotificationSourceKind::TickTick),
            Self::API => Some(NotificationSourceKind::API),
        }
    }
}
