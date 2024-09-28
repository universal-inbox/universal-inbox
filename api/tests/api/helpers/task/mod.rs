use reqwest::{Client, Response};
use serde_json::json;

use universal_inbox::{
    task::{
        ProjectSummary, Task, TaskCreationResult, TaskId, TaskSourceKind, TaskStatus, TaskSummary,
    },
    Page,
};

pub mod linear;
pub mod todoist;

pub async fn list_tasks_response(
    client: &Client,
    api_address: &str,
    status_filter: TaskStatus,
    trigger_sync: bool,
    only_synced_tasks: Option<bool>,
) -> Response {
    let params = only_synced_tasks.map_or("".to_string(), |synced| {
        format!("&only_synced_tasks={}", synced)
    });

    client
        .get(&format!(
            "{api_address}tasks?status={status_filter}&trigger_sync={trigger_sync}{params}"
        ))
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn list_synced_tasks(
    client: &Client,
    api_address: &str,
    status_filter: TaskStatus,
    trigger_sync: bool,
) -> Vec<Task> {
    let tasks: Page<Task> =
        list_tasks_response(client, api_address, status_filter, trigger_sync, Some(true))
            .await
            .json()
            .await
            .expect("Cannot parse JSON result");
    tasks.content
}

pub async fn list_tasks(
    client: &Client,
    api_address: &str,
    status_filter: TaskStatus,
    trigger_sync: bool,
) -> Vec<Task> {
    let tasks: Page<Task> =
        list_tasks_response(client, api_address, status_filter, trigger_sync, None)
            .await
            .json()
            .await
            .expect("Cannot parse JSON result");
    tasks.content
}

pub async fn get_task_response(client: &Client, api_address: &str, task_id: TaskId) -> Response {
    client
        .get(&format!("{api_address}tasks/{task_id}"))
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn get_task(client: &Client, api_address: &str, task_id: TaskId) -> Option<Task> {
    get_task_response(client, api_address, task_id)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}

pub async fn search_tasks_response(client: &Client, api_address: &str, matches: &str) -> Response {
    client
        .get(&format!("{api_address}tasks/search?matches={matches}"))
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn search_tasks(client: &Client, api_address: &str, matches: &str) -> Vec<TaskSummary> {
    search_tasks_response(client, api_address, matches)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}

pub async fn sync_tasks_response(
    client: &Client,
    api_address: &str,
    source: Option<TaskSourceKind>,
    asynchronous: bool,
) -> Response {
    client
        .post(&format!("{api_address}tasks/sync"))
        .json(
            &source
                .map(|src| json!({"source": src.to_string(), "asynchronous": asynchronous}))
                .unwrap_or_else(|| json!({ "asynchronous": asynchronous })),
        )
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn sync_tasks(
    client: &Client,
    api_address: &str,
    source: Option<TaskSourceKind>,
    asynchronous: bool,
) -> Vec<TaskCreationResult> {
    sync_tasks_response(client, api_address, source, asynchronous)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}

pub async fn search_projects_response(
    client: &Client,
    api_address: &str,
    matches: &str,
) -> Response {
    client
        .get(&format!(
            "{api_address}tasks/projects/search?matches={matches}"
        ))
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn search_projects(
    client: &Client,
    api_address: &str,
    matches: &str,
) -> Vec<ProjectSummary> {
    search_projects_response(client, api_address, matches)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}
