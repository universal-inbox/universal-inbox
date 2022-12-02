use reqwest::Response;

use universal_inbox::task::{Task, TaskStatus};

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
