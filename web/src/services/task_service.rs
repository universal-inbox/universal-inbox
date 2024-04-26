use anyhow::Result;
use dioxus::prelude::*;

use futures_util::StreamExt;
use reqwest::Method;
use url::Url;

use universal_inbox::task::{
    service::SyncTasksParameters, service::TaskPatch, Task, TaskCreationResult, TaskId,
    TaskPlanning, TaskStatus, TaskSyncSourceKind,
};

use crate::{
    model::UniversalInboxUIModel,
    services::{api::call_api_and_notify, toast_service::ToastCommand},
};

#[derive(Debug)]
pub enum TaskCommand {
    Delete(TaskId),
    Complete(TaskId),
    Plan(TaskId, TaskPlanning),
    Sync(Option<TaskSyncSourceKind>),
}

pub async fn task_service(
    mut rx: UnboundedReceiver<TaskCommand>,
    api_base_url: Url,
    ui_model: Signal<UniversalInboxUIModel>,
    toast_service: Coroutine<ToastCommand>,
) {
    loop {
        let msg = rx.next().await;

        match msg {
            Some(TaskCommand::Delete(task_id)) => {
                let _result: Result<Task> = call_api_and_notify(
                    Method::PATCH,
                    &api_base_url,
                    &format!("tasks/{task_id}"),
                    Some(TaskPatch {
                        status: Some(TaskStatus::Deleted),
                        ..Default::default()
                    }),
                    Some(ui_model),
                    &toast_service,
                    "Deleting task...",
                    "Successfully deleted task",
                )
                .await;
            }
            Some(TaskCommand::Complete(task_id)) => {
                let _result: Result<Task> = call_api_and_notify(
                    Method::PATCH,
                    &api_base_url,
                    &format!("tasks/{task_id}"),
                    Some(TaskPatch {
                        status: Some(TaskStatus::Done),
                        ..Default::default()
                    }),
                    Some(ui_model),
                    &toast_service,
                    "Completing task...",
                    "Successfully completed task",
                )
                .await;
            }
            Some(TaskCommand::Plan(task_id, parameters)) => {
                let _result: Result<Task> = call_api_and_notify(
                    Method::PATCH,
                    &api_base_url,
                    &format!("tasks/{task_id}"),
                    Some(TaskPatch {
                        project: Some(parameters.project.to_string()),
                        due_at: Some(parameters.due_at),
                        priority: Some(parameters.priority),
                        ..Default::default()
                    }),
                    Some(ui_model),
                    &toast_service,
                    "Planning task...",
                    "Successfully planned task",
                )
                .await;
            }
            Some(TaskCommand::Sync(source)) => {
                let _result: Result<Vec<TaskCreationResult>> = call_api_and_notify(
                    Method::POST,
                    &api_base_url,
                    "tasks/sync",
                    Some(SyncTasksParameters {
                        source,
                        asynchronous: Some(false),
                    }),
                    Some(ui_model),
                    &toast_service,
                    "Syncing tasks...",
                    "Successfully synced tasks",
                )
                .await;
            }
            None => (),
        }
    }
}
