use serde::{Deserialize, Serialize};

use crate::{
    task::{DueDate, TaskPriority, TaskStatus, TaskSyncSourceKind},
    third_party::item::ThirdPartyItemId,
};

#[derive(Serialize, Deserialize, Debug)]
pub struct SyncTasksParameters {
    pub source: Option<TaskSyncSourceKind>,
    pub asynchronous: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Eq)]
pub struct TaskPatch {
    pub status: Option<TaskStatus>,
    pub project_name: Option<String>,
    pub due_at: Option<Option<DueDate>>,
    pub priority: Option<TaskPriority>,
    pub body: Option<String>,
    pub title: Option<String>,
    pub sink_item_id: Option<ThirdPartyItemId>,
}
