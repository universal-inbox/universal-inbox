use std::collections::HashMap;

use dioxus::prelude::*;
use futures_util::StreamExt;

use universal_inbox::task::{Task, TaskId, TaskPatch, TaskPlanning, TaskStatus};

use crate::services::{api::call_api_and_notify, toast_service::ToastCommand};

#[derive(Debug)]
pub enum TaskCommand {
    Delete(TaskId),
    Complete(TaskId),
    Plan(TaskId, TaskPlanning),
}

pub async fn task_service<'a>(
    mut rx: UnboundedReceiver<TaskCommand>,
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
            Some(TaskCommand::Plan(task_id, parameters)) => {
                let _result: Task = call_api_and_notify(
                    "PATCH",
                    &format!("/tasks/{}", task_id),
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
            None => (),
        }
    }
}
