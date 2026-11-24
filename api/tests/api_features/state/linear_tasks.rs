use universal_inbox::task::{Task, TaskCreationResult};

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum Reported {
    #[default]
    TheIssue,
    NoIssue,
}

#[derive(Default)]
pub struct LinearTasks {
    pub reported: Reported,
    pub title: Option<String>,
    pub description: Option<String>,
    pub new_activity: bool,
    pub task: Option<Task>,
    pub results: Vec<TaskCreationResult>,
}
