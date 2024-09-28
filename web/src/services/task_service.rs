use anyhow::Result;
use dioxus::prelude::*;

use futures_util::StreamExt;
use reqwest::Method;
use url::Url;

use universal_inbox::{
    task::{
        service::{SyncTasksParameters, TaskPatch},
        Task, TaskCreationResult, TaskId, TaskPlanning, TaskStatus, TaskSyncSourceKind,
    },
    Page,
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

pub static SYNCED_TASKS_PAGE: GlobalSignal<Page<Task>> = Signal::global(|| Page {
    page: 0,
    per_page: 0,
    total: 0,
    content: vec![],
});

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
    mut ui_model: Signal<UniversalInboxUIModel>,
) {
    ui_model.write().synced_tasks_count = None;

    let result: Result<Page<Task>> = call_api(
        Method::GET,
        api_base_url,
        "tasks?status=Active&only_synced_tasks=true",
        None::<i32>,
        Some(ui_model),
    )
    .await;

    match result {
        Ok(new_synced_tasks_page) => {
            ui_model.write().synced_tasks_count = Some(Ok(new_synced_tasks_page.total));
            *synced_tasks_page.write() = new_synced_tasks_page;
        }
        Err(err) => {
            ui_model.write().synced_tasks_count =
                Some(Err(format!("Failed to load synchronized tasks: {err}")));
        }
    }
}

async fn complete_task(
    api_base_url: &Url,
    task_id: TaskId,
    mut synced_tasks_page: Signal<Page<Task>>,
    mut ui_model: Signal<UniversalInboxUIModel>,
    toast_service: Coroutine<ToastCommand>,
) {
    {
        let mut synced_tasks_page = synced_tasks_page.write();
        let mut ui_model = ui_model.write();

        synced_tasks_page.content.retain(|t| t.id != task_id);
        let synced_tasks_count = synced_tasks_page.content.len();

        if synced_tasks_count > 0 && ui_model.selected_notification_index >= synced_tasks_count {
            ui_model.selected_notification_index = synced_tasks_count - 1;
        }
    }

    let _result: Result<Task> = call_api_and_notify(
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
