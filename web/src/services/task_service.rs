use std::{
    collections::HashMap,
    fmt::{Display, Formatter},
    str::FromStr,
};

use dioxus::prelude::*;
use fermi::{AtomRef, UseAtomRef};
use futures_util::StreamExt;
use log::debug;

use universal_inbox::task::{DueDate, Task, TaskId, TaskPatch, TaskPriority, TaskStatus};

use crate::services::{api::call_api_and_notify, toast_service::ToastCommand};

#[derive(Debug)]
pub enum TaskCommand {
    UpdateTasks(HashMap<TaskId, Task>),
    Delete(TaskId),
    Complete(TaskId),
    Plan(TaskPlanningParameters),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaskProject(String);

impl Display for TaskProject {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for TaskProject {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() {
            Err("Task's project is required".to_string())
        } else if value == "Inbox" {
            Err("Task's project must be moved out of the inbox".to_string())
        } else {
            Ok(TaskProject(value.to_string()))
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaskPlanningParameters {
    pub task_id: TaskId,
    pub project: TaskProject,
    pub due_at: Option<DueDate>,
    pub priority: TaskPriority,
}

pub static TASKS: AtomRef<HashMap<TaskId, Task>> = |_| HashMap::new();

pub async fn task_service<'a>(
    mut rx: UnboundedReceiver<TaskCommand>,
    tasks: UseAtomRef<HashMap<TaskId, Task>>,
    toast_service: Coroutine<ToastCommand>,
) {
    loop {
        let msg = rx.next().await;

        match msg {
            Some(TaskCommand::Delete(task_id)) => {
                let _result: Task = call_api_and_notify(
                    "PATCH",
                    &format!("/tasks/{}", task_id),
                    TaskPatch {
                        status: Some(TaskStatus::Deleted),
                        ..Default::default()
                    },
                    HashMap::new(),
                    &toast_service,
                    "Deleting task...",
                    "Successfully deleted task",
                )
                .await
                .unwrap();
            }
            Some(TaskCommand::Complete(task_id)) => {
                let _result: Task = call_api_and_notify(
                    "PATCH",
                    &format!("/tasks/{}", task_id),
                    TaskPatch {
                        status: Some(TaskStatus::Done),
                        ..Default::default()
                    },
                    HashMap::new(),
                    &toast_service,
                    "Completing task...",
                    "Successfully completed task",
                )
                .await
                .unwrap();
            }
            Some(TaskCommand::Plan(parameters)) => {
                let _result: Task = call_api_and_notify(
                    "PATCH",
                    &format!("/tasks/{}", parameters.task_id),
                    TaskPatch {
                        project: Some(parameters.project.to_string()),
                        due_at: Some(parameters.due_at),
                        priority: Some(parameters.priority),
                        ..Default::default()
                    },
                    HashMap::new(),
                    &toast_service,
                    "Planning task...",
                    "Successfully planned task",
                )
                .await
                .unwrap();
            }
            Some(TaskCommand::UpdateTasks(new_tasks)) => {
                debug!("{} tasks loaded", new_tasks.len());
                tasks.write().extend(new_tasks);
            }
            None => (),
        }
    }
}
