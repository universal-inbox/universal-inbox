use reqwest::Response;
use serde_json::json;

use universal_inbox::task::{Task, TaskStatus};
use universal_inbox_api::universal_inbox::task::source::TaskSourceKind;

pub mod todoist;

pub async fn list_tasks_response(app_address: &str, status_filter: TaskStatus) -> Response {
    reqwest::Client::new()
        .get(&format!("{app_address}/tasks?status={status_filter}"))
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn list_tasks(app_address: &str, status_filter: TaskStatus) -> Vec<Task> {
    list_tasks_response(app_address, status_filter)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}

pub async fn sync_tasks_response(app_address: &str, source: Option<TaskSourceKind>) -> Response {
    reqwest::Client::new()
        .post(&format!("{}/tasks/sync", &app_address))
        .json(
            &source
                .map(|src| json!({"source": src.to_string()}))
                .unwrap_or_else(|| json!({})),
        )
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn sync_tasks(app_address: &str, source: Option<TaskSourceKind>) -> Vec<Task> {
    sync_tasks_response(app_address, source)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}
