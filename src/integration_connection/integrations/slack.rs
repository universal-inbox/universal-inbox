use serde::{Deserialize, Serialize};

use crate::task::{PresetDueDate, TaskPriority};

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct SlackConfig {
    pub sync_enabled: bool,
    pub sync_type: SlackSyncType,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
#[serde(tag = "type", content = "content")]
pub enum SlackSyncType {
    AsNotifications,
    AsTasks(SlackSyncTaskConfig),
}

impl Default for SlackConfig {
    fn default() -> Self {
        Self {
            sync_enabled: true,
            sync_type: SlackSyncType::AsNotifications,
        }
    }
}

impl SlackConfig {
    pub fn enabled() -> Self {
        Self {
            sync_enabled: true,
            sync_type: SlackSyncType::AsNotifications,
        }
    }

    pub fn disabled() -> Self {
        Self {
            sync_enabled: false,
            sync_type: SlackSyncType::AsNotifications,
        }
    }
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct SlackSyncTaskConfig {
    pub target_project: String,
    pub default_due_at: Option<PresetDueDate>,
    pub default_priority: TaskPriority,
}

impl Default for SlackSyncTaskConfig {
    fn default() -> Self {
        Self {
            target_project: "Inbox".to_string(),
            default_due_at: None,
            default_priority: TaskPriority::P4,
        }
    }
}
