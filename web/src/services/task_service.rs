use anyhow::Result;
use dioxus::prelude::*;

use futures_util::StreamExt;
use reqwest::Method;
use url::Url;

use universal_inbox::{
    Page,
    task::{
        Task, TaskCreationResult, TaskId, TaskPlanning, TaskStatus, TaskSyncSourceKind,
        service::{SyncTasksParameters, TaskPatch},
    },
};

use crate::{
    model::UniversalInboxUIModel,
    services::{
        api::{call_api, call_api_and_notify},
        toast_service::ToastCommand,
    },
};

#[derive(Debug, PartialEq)]
pub enum TaskCommand {
    RefreshSyncedTasks,
    Delete(TaskId),
    Complete(TaskId),
    Plan(TaskId, TaskPlanning),
    Sync(Option<TaskSyncSourceKind>),
}

pub static SYNCED_TASKS_PAGE: GlobalSignal<Page<Task>> = Signal::global(Page::default);

pub async fn task_service(
    mut rx: UnboundedReceiver<TaskCommand>,
    api_base_url: Url,
    synced_tasks_page: Signal<Page<Task>>,
    ui_model: Signal<UniversalInboxUIModel>,
    toast_service: Coroutine<ToastCommand>,
) {
    loop {
        let msg = rx.next().await;

        match msg {
            Some(TaskCommand::Delete(task_id)) => {
                let _result: Result<Option<Task>> = call_api_and_notify(
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
                complete_task(
                    &api_base_url,
                    task_id,
                    synced_tasks_page,
                    ui_model,
                    toast_service,
                )
                .await;
            }

            Some(TaskCommand::Plan(task_id, parameters)) => {
                let _result: Result<Option<Task>> = call_api_and_notify(
                    Method::PATCH,
                    &api_base_url,
                    &format!("tasks/{task_id}"),
                    Some(TaskPatch {
                        project_name: Some(parameters.project_name),
                        due_at: Some(parameters.due_at),
                        priority: Some(parameters.priority),
                        status: Some(TaskStatus::Active),
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

            Some(TaskCommand::RefreshSyncedTasks) => {
                refresh_synced_tasks(&api_base_url, synced_tasks_page, ui_model).await;
            }

            None => (),
        }
    }
}

async fn refresh_synced_tasks(
    api_base_url: &Url,
    mut synced_tasks_page: Signal<Page<Task>>,
    ui_model: Signal<UniversalInboxUIModel>,
) {
    let result: Result<Page<Task>> = call_api(
        Method::GET,
        api_base_url,
        "tasks?status=Active&only_synced_tasks=true",
        None::<i32>,
        Some(ui_model),
    )
    .await;

    if let Ok(new_synced_tasks_page) = result {
        *synced_tasks_page.write() = new_synced_tasks_page;
    }
}

async fn complete_task(
    api_base_url: &Url,
    task_id: TaskId,
    mut synced_tasks_page: Signal<Page<Task>>,
    ui_model: Signal<UniversalInboxUIModel>,
    toast_service: Coroutine<ToastCommand>,
) {
    synced_tasks_page
        .write()
        .remove_element(|t| t.id != task_id);

    let _result: Result<Option<Task>> = call_api_and_notify(
        Method::PATCH,
        api_base_url,
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
