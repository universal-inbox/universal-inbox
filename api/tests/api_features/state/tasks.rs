use std::collections::HashMap;

use uuid::Uuid;

use universal_inbox::task::{ProjectSummary, Task, TaskSummary};

#[derive(Default)]
pub struct Tasks {
    pub by_name: HashMap<String, Task>,
    pub created: usize,
    pub listed: Vec<Task>,
    pub found: Vec<TaskSummary>,
    pub projects: Vec<ProjectSummary>,
    pub unknown_id: Option<Uuid>,
}

impl Tasks {
    pub fn named(&self, name: &str) -> &Task {
        self.by_name
            .get(name)
            .unwrap_or_else(|| panic!("No task is named {name:?}"))
    }
}
