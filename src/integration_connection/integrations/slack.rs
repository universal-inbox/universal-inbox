use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use slack_morphism::{SlackReactionName, SlackTeamId};

use crate::task::{PresetDueDate, ProjectSummary, TaskPriority};

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct SlackConfig {
    pub reaction_config: SlackReactionConfig,
    pub message_config: SlackMessageConfig,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct SlackReactionConfig {
    pub sync_enabled: bool,
    pub reaction_name: SlackReactionName,
    pub sync_type: SlackSyncType,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct SlackMessageConfig {
    pub sync_enabled: bool,
    // 2way sync is not really possible with current Slack public API
    // Keeping it for now as the logic is already implemented and it
    // might be possible to workaround this limitation in the future
    pub is_2way_sync: bool,
    #[serde(default)]
    pub extension_enabled: bool,
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
            reaction_config: SlackReactionConfig {
                sync_enabled: false,
                reaction_name: SlackReactionName("eyes".to_string()),
                sync_type: SlackSyncType::AsNotifications,
            },
            message_config: SlackMessageConfig {
                sync_enabled: false,
                is_2way_sync: false,
                extension_enabled: true,
            },
        }
    }
}

impl SlackConfig {
    pub fn enabled_as_notifications() -> Self {
        Self {
            reaction_config: SlackReactionConfig {
                sync_enabled: true,
                reaction_name: SlackReactionName("eyes".to_string()),
                sync_type: SlackSyncType::AsNotifications,
            },
            message_config: SlackMessageConfig {
                sync_enabled: true,
                is_2way_sync: false,
                extension_enabled: true,
            },
        }
    }

    pub fn enabled_as_tasks() -> Self {
        Self {
            reaction_config: SlackReactionConfig {
                sync_enabled: true,
                reaction_name: SlackReactionName("eyes".to_string()),
                sync_type: SlackSyncType::AsTasks(SlackSyncTaskConfig::default()),
            },
            message_config: SlackMessageConfig {
                sync_enabled: false,
                is_2way_sync: false,
                extension_enabled: true,
            },
        }
    }

    pub fn disabled() -> Self {
        Self {
            reaction_config: SlackReactionConfig {
                sync_enabled: false,
                reaction_name: SlackReactionName("eyes".to_string()),
                sync_type: SlackSyncType::AsNotifications,
            },
            message_config: SlackMessageConfig {
                sync_enabled: false,
                is_2way_sync: false,
                extension_enabled: false,
            },
        }
    }
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct SlackSyncTaskConfig {
    pub target_project: Option<ProjectSummary>,
    pub default_due_at: Option<PresetDueDate>,
    pub default_priority: TaskPriority,
}

impl Default for SlackSyncTaskConfig {
    fn default() -> Self {
        Self {
            target_project: None,
            default_due_at: None,
            default_priority: TaskPriority::P4,
        }
    }
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct SlackExtensionCredential {
    pub team_id: SlackTeamId,
    pub user_id: String,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct SlackContext {
    pub team_id: SlackTeamId,
    #[serde(default)]
    pub extension_credentials: Vec<SlackExtensionCredential>,
    #[serde(default)]
    pub last_extension_heartbeat_at: Option<DateTime<Utc>>,
}
