use serde::{Deserialize, Serialize};

use crate::task::{DueDate, TaskPriority, TaskStatus, TaskSyncSourceKind};

#[derive(Serialize, Deserialize, Debug)]
pub struct SyncTasksParameters {
    pub source: Option<TaskSyncSourceKind>,
    pub asynchronous: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Eq)]
pub struct TaskPatch {
    pub status: Option<TaskStatus>,
    pub project: Option<String>,
    pub due_at: Option<Option<DueDate>>,
    pub priority: Option<TaskPriority>,
    pub body: Option<String>,
}
