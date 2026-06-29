use serde::{Deserialize, Serialize};

use crate::integration_connection::integrations::task_time_config::TaskTimeConfig;
use crate::integration_connection::provider::IntegrationProviderKind;
use crate::task::{PresetDueDate, ProjectSummary};

fn default_true() -> bool {
    true
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct LinearConfig {
    pub sync_notifications_enabled: bool,
    pub sync_task_config: LinearSyncTaskConfig,
}

impl Default for LinearConfig {
    fn default() -> Self {
        Self {
            sync_notifications_enabled: true,
            sync_task_config: LinearSyncTaskConfig::default(),
        }
    }
}

impl LinearConfig {
    pub fn enabled() -> Self {
        Self {
            sync_notifications_enabled: true,
            sync_task_config: LinearSyncTaskConfig::enabled(),
        }
    }

    pub fn disabled() -> Self {
        Self {
            sync_notifications_enabled: false,
            sync_task_config: LinearSyncTaskConfig::disabled(),
        }
    }
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct LinearSyncTaskConfig {
    pub enabled: bool,
    pub target_project: Option<ProjectSummary>,
    pub default_due_at: Option<PresetDueDate>,
    #[serde(default = "default_true")]
    pub auto_delete_notifications: bool,
    pub task_manager_provider_kind: Option<IntegrationProviderKind>,
    #[serde(default)]
    pub default_time_config: Option<TaskTimeConfig>,
}

impl Default for LinearSyncTaskConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            target_project: None,
            default_due_at: None,
            auto_delete_notifications: true,
            task_manager_provider_kind: None,
            default_time_config: None,
        }
    }
}

impl LinearSyncTaskConfig {
    pub fn enabled() -> Self {
        Self {
            enabled: true,
            target_project: None,
            default_due_at: None,
            auto_delete_notifications: true,
            task_manager_provider_kind: None,
            default_time_config: None,
        }
    }

    pub fn disabled() -> Self {
        Self {
            enabled: false,
            target_project: None,
            default_due_at: None,
            auto_delete_notifications: false,
            task_manager_provider_kind: None,
            default_time_config: None,
        }
    }
}
