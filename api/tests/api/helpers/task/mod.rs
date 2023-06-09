use reqwest::{Client, Response};
use serde_json::json;

use universal_inbox::task::{Task, TaskSourceKind, TaskStatus, TaskSummary};

use universal_inbox_api::universal_inbox::task::TaskCreationResult;

pub mod todoist;

pub async fn list_tasks_response(
    client: &Client,
    app_address: &str,
    status_filter: TaskStatus,
) -> Response {
    client
        .get(&format!("{app_address}/tasks?status={status_filter}"))
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn list_tasks(
    client: &Client,
    app_address: &str,
    status_filter: TaskStatus,
) -> Vec<Task> {
    list_tasks_response(client, app_address, status_filter)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}

pub async fn search_tasks_response(client: &Client, app_address: &str, matches: &str) -> Response {
    client
        .get(&format!("{app_address}/tasks/search?matches={matches}"))
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn search_tasks(client: &Client, app_address: &str, matches: &str) -> Vec<TaskSummary> {
    search_tasks_response(client, app_address, matches)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}

pub async fn sync_tasks_response(
    client: &Client,
    app_address: &str,
    source: Option<TaskSourceKind>,
    asynchronous: bool,
) -> Response {
    client
        .post(&format!("{}/tasks/sync", &app_address))
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
    app_address: &str,
    source: Option<TaskSourceKind>,
    asynchronous: bool,
) -> Vec<TaskCreationResult> {
    sync_tasks_response(client, app_address, source, asynchronous)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}
