use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::task::{PresetDueDate, ProjectSummary, TaskPriority};

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct TickTickContext {
    /// TickTick V1 API has no incremental sync token.
    /// Track last successful sync timestamp for freshness checks.
    pub last_sync_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct TickTickConfig {
    pub sync_tasks_enabled: bool,
    pub create_notification_from_inbox_task: bool,
    pub default_project: Option<ProjectSummary>,
    pub default_due_at: Option<PresetDueDate>,
    pub default_priority: Option<TaskPriority>,
}

impl Default for TickTickConfig {
    fn default() -> Self {
        Self {
            sync_tasks_enabled: true,
            create_notification_from_inbox_task: false,
            default_project: None,
            default_due_at: None,
            default_priority: None,
        }
    }
}

impl TickTickConfig {
    pub fn enabled() -> Self {
        Self {
            sync_tasks_enabled: true,
            create_notification_from_inbox_task: true,
            default_project: None,
            default_due_at: None,
            default_priority: None,
        }
    }

    pub fn disabled() -> Self {
        Self {
            sync_tasks_enabled: false,
            create_notification_from_inbox_task: false,
            default_project: None,
            default_due_at: None,
            default_priority: None,
        }
    }
}
