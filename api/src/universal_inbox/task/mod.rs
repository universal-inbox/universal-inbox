use serde::{Deserialize, Serialize};
use universal_inbox::{notification::Notification, task::Task};

pub mod service;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub struct TaskCreationResult {
    pub task: Task,
    pub notification: Option<Notification>,
}
