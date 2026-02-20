use serde::{Deserialize, Serialize};

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
}

impl Default for LinearSyncTaskConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            target_project: None,
            default_due_at: None,
            auto_delete_notifications: true,
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
        }
    }

    pub fn disabled() -> Self {
        Self {
            enabled: false,
            target_project: None,
            default_due_at: None,
            auto_delete_notifications: false,
        }
    }
}
