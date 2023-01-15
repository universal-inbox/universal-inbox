use std::collections::HashMap;

use dioxus::prelude::*;
use futures_util::StreamExt;

use universal_inbox::task::{Task, TaskId, TaskPatch, TaskStatus};

use crate::services::{api::call_api_and_notify, toast_service::ToastCommand};

#[derive(Debug)]
pub enum TaskCommand {
    Delete(TaskId),
    Complete(TaskId),
}

pub async fn task_service<'a>(
    mut rx: UnboundedReceiver<TaskCommand>,
    toast_service: CoroutineHandle<ToastCommand>,
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
                    },
                    HashMap::new(),
                    &toast_service,
                    "Completing task...",
                    "Successfully completed task",
                )
                .await
                .unwrap();
            }
            None => (),
        }
    }
}
