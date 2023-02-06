#![allow(clippy::useless_conversion)]

use reqwest::Response;
use serde_json::json;

use universal_inbox::{
    notification::{Notification, NotificationStatus},
    task::TaskId,
    NotificationsListResult,
};

use universal_inbox_api::integrations::notification::NotificationSourceKind;

pub mod github;

pub async fn list_notifications_response(
    app_address: &str,
    status_filter: NotificationStatus,
    include_snoozed_notifications: bool,
    task_id: Option<TaskId>,
    load_tasks: bool,
) -> Response {
    let snoozed_notifications_parameter = if include_snoozed_notifications {
        "&include_snoozed_notifications=true"
    } else {
        ""
    };
    let task_id_parameter = task_id
        .map(|id| format!("&task_id={id}"))
        .unwrap_or_default();
    let with_tasks_parameter = if load_tasks { "&with_tasks=true" } else { "" };

    reqwest::Client::new()
        .get(&format!(
            "{app_address}/notifications?status={status_filter}{snoozed_notifications_parameter}{task_id_parameter}{with_tasks_parameter}"
        ))
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn list_notifications(
    app_address: &str,
    status_filter: NotificationStatus,
    include_snoozed_notifications: bool,
    task_id: Option<TaskId>,
    load_tasks: bool,
) -> NotificationsListResult {
    list_notifications_response(
        app_address,
        status_filter,
        include_snoozed_notifications,
        task_id,
        load_tasks,
    )
    .await
    .json()
    .await
    .expect("Cannot parse JSON result")
}

pub async fn sync_notifications_response(
    app_address: &str,
    source: Option<NotificationSourceKind>,
) -> Response {
    reqwest::Client::new()
        .post(&format!("{}/notifications/sync", &app_address))
        .json(
            &source
                .map(|src| json!({"source": src.to_string()}))
                .unwrap_or_else(|| json!({})),
        )
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn sync_notifications(
    app_address: &str,
    source: Option<NotificationSourceKind>,
) -> Vec<Notification> {
    sync_notifications_response(app_address, source)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}
