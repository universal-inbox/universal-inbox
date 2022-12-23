use std::collections::HashMap;

use dioxus::prelude::*;
use futures_util::StreamExt;

use universal_inbox::task::{Task, TaskId, TaskPatch, TaskStatus};

use crate::services::{api::call_api_and_notify, toast_service::ToastCommand};

#[derive(Debug)]
pub enum TaskCommand {
    Delete(TaskId),
}

pub async fn task_service<'a>(
    mut rx: UnboundedReceiver<TaskCommand>,
    toast_service: CoroutineHandle<ToastCommand>,
) {
    loop {
        let msg = rx.next().await;

        if let Some(TaskCommand::Delete(task_id)) = msg {
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
    }
}
