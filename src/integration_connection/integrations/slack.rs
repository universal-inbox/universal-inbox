use serde::{Deserialize, Serialize};
use slack_morphism::{SlackReactionName, SlackTeamId};

use crate::task::{PresetDueDate, ProjectSummary, TaskPriority};

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct SlackConfig {
    pub star_config: SlackStarConfig,
    pub reaction_config: SlackReactionConfig,
    pub message_config: SlackMessageConfig,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct SlackStarConfig {
    pub sync_enabled: bool,
    pub sync_type: SlackSyncType,
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
            star_config: SlackStarConfig {
                sync_enabled: false,
                sync_type: SlackSyncType::AsNotifications,
            },
            reaction_config: SlackReactionConfig {
                sync_enabled: false,
                reaction_name: SlackReactionName("eyes".to_string()),
                sync_type: SlackSyncType::AsNotifications,
            },
            message_config: SlackMessageConfig {
                sync_enabled: false,
                is_2way_sync: false,
            },
        }
    }
}

impl SlackConfig {
    pub fn enabled_as_notifications() -> Self {
        Self {
            star_config: SlackStarConfig {
                sync_enabled: true,
                sync_type: SlackSyncType::AsNotifications,
            },
            reaction_config: SlackReactionConfig {
                sync_enabled: true,
                reaction_name: SlackReactionName("eyes".to_string()),
                sync_type: SlackSyncType::AsNotifications,
            },
            message_config: SlackMessageConfig {
                sync_enabled: true,
                is_2way_sync: false,
            },
        }
    }

    pub fn enabled_as_tasks() -> Self {
        Self {
            star_config: SlackStarConfig {
                sync_enabled: true,
                sync_type: SlackSyncType::AsTasks(SlackSyncTaskConfig::default()),
            },
            reaction_config: SlackReactionConfig {
                sync_enabled: true,
                reaction_name: SlackReactionName("eyes".to_string()),
                sync_type: SlackSyncType::AsTasks(SlackSyncTaskConfig::default()),
            },
            message_config: SlackMessageConfig {
                sync_enabled: false,
                is_2way_sync: false,
            },
        }
    }

    pub fn disabled() -> Self {
        Self {
            star_config: SlackStarConfig {
                sync_enabled: false,
                sync_type: SlackSyncType::AsNotifications,
            },
            reaction_config: SlackReactionConfig {
                sync_enabled: false,
                reaction_name: SlackReactionName("eyes".to_string()),
                sync_type: SlackSyncType::AsNotifications,
            },
            message_config: SlackMessageConfig {
                sync_enabled: false,
                is_2way_sync: false,
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
pub struct SlackContext {
    pub team_id: SlackTeamId,
}
