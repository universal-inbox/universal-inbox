use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::{
    task::{
        DueDate, TASK_BODY_MAX_LENGTH, TASK_PROJECT_NAME_MAX_LENGTH, TASK_TITLE_MAX_LENGTH,
        TaskPriority, TaskStatus, TaskSyncSourceKind,
    },
    third_party::item::ThirdPartyItemId,
};

#[derive(Serialize, Deserialize, Debug)]
pub struct SyncTasksParameters {
    pub source: Option<TaskSyncSourceKind>,
    pub asynchronous: Option<bool>,
}

/// Changes to a task. Callers validate it (`Validate`) at the request
/// boundary (HTTP, MCP); internal sync flows build it unchecked.
#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Eq, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct TaskPatch {
    pub status: Option<TaskStatus>,
    #[validate(length(max = TASK_PROJECT_NAME_MAX_LENGTH))]
    pub project_name: Option<String>,
    pub due_at: Option<Option<DueDate>>,
    pub priority: Option<TaskPriority>,
    #[validate(length(max = TASK_BODY_MAX_LENGTH))]
    pub body: Option<String>,
    #[validate(length(min = 1, max = TASK_TITLE_MAX_LENGTH))]
    pub title: Option<String>,
    pub sink_item_id: Option<ThirdPartyItemId>,
}
