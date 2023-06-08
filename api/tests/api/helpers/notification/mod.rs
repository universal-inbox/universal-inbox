#![allow(clippy::useless_conversion)]

use reqwest::{Client, Response};
use serde_json::json;

use universal_inbox::{
    notification::{Notification, NotificationId, NotificationStatus, NotificationWithTask},
    task::{TaskCreation, TaskId},
};

use universal_inbox::notification::NotificationSourceKind;

pub mod github;

pub async fn list_notifications_response(
    client: &Client,
    app_address: &str,
    status_filter: NotificationStatus,
    include_snoozed_notifications: bool,
    task_id: Option<TaskId>,
) -> Response {
    let snoozed_notifications_parameter = if include_snoozed_notifications {
        "&include_snoozed_notifications=true"
    } else {
        ""
    };
    let task_id_parameter = task_id
        .map(|id| format!("&task_id={id}"))
        .unwrap_or_default();

    client
        .get(&format!(
            "{app_address}/notifications?status={status_filter}{snoozed_notifications_parameter}{task_id_parameter}"
        ))
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn list_notifications_with_tasks(
    client: &Client,
    app_address: &str,
    status_filter: NotificationStatus,
    include_snoozed_notifications: bool,
    task_id: Option<TaskId>,
) -> Vec<NotificationWithTask> {
    list_notifications_response(
        client,
        app_address,
        status_filter,
        include_snoozed_notifications,
        task_id,
    )
    .await
    .json()
    .await
    .expect("Cannot parse JSON result")
}

pub async fn list_notifications(
    client: &Client,
    app_address: &str,
    status_filter: NotificationStatus,
    include_snoozed_notifications: bool,
    task_id: Option<TaskId>,
) -> Vec<Notification> {
    list_notifications_with_tasks(
        client,
        app_address,
        status_filter,
        include_snoozed_notifications,
        task_id,
    )
    .await
    .into_iter()
    .map(|n| n.into())
    .collect()
}

pub async fn sync_notifications_response(
    client: &Client,
    app_address: &str,
    source: Option<NotificationSourceKind>,
    asynchronous: bool,
) -> Response {
    client
        .post(&format!("{}/notifications/sync", &app_address))
        .json(
            &source
                .map(|src| {
                    json!({
                        "source": src.to_string(),
                        "asynchronous": asynchronous,
                    })
                })
                .unwrap_or_else(|| json!({ "asynchronous": asynchronous })),
        )
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn sync_notifications(
    client: &Client,
    app_address: &str,
    source: Option<NotificationSourceKind>,
    asynchronous: bool,
) -> Vec<Notification> {
    sync_notifications_response(client, app_address, source, asynchronous)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}

pub async fn create_task_from_notification_response(
    client: &Client,
    app_address: &str,
    notification_id: NotificationId,
    task_creation: &TaskCreation,
) -> Response {
    client
        .post(&format!(
            "{}/notifications/{}/task",
            &app_address, notification_id
        ))
        .json(task_creation)
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn create_task_from_notification(
    client: &Client,
    app_address: &str,
    notification_id: NotificationId,
    task_creation: &TaskCreation,
) -> Option<NotificationWithTask> {
    create_task_from_notification_response(client, app_address, notification_id, task_creation)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}
