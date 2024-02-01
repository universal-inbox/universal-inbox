#![allow(clippy::useless_conversion)]

use reqwest::{Client, Response};
use serde_json::json;

use universal_inbox::{
    notification::{
        Notification, NotificationDetails, NotificationId, NotificationSourceKind,
        NotificationStatus, NotificationWithTask,
    },
    task::{TaskCreation, TaskId},
    Page,
};

use universal_inbox_api::repository::notification::NotificationRepository;

use crate::helpers::auth::AuthenticatedApp;

pub mod github;
pub mod google_mail;
pub mod linear;

pub async fn list_notifications_response(
    client: &Client,
    api_address: &str,
    status_filter: Vec<NotificationStatus>,
    include_snoozed_notifications: bool,
    task_id: Option<TaskId>,
    notification_kind: Option<NotificationSourceKind>,
) -> Response {
    let snoozed_notifications_parameter = if include_snoozed_notifications {
        "include_snoozed_notifications=true&"
    } else {
        ""
    };
    let task_id_parameter = task_id
        .map(|id| format!("task_id={id}&"))
        .unwrap_or_default();
    let status_parameter = if status_filter.is_empty() {
        "".to_string()
    } else {
        let filters = status_filter
            .into_iter()
            .map(|s| s.to_string())
            .collect::<Vec<String>>()
            .join(",");
        format!("status={filters}&")
    };
    let notification_kind_parameter = notification_kind
        .map(|kind| format!("notification_kind={kind}&"))
        .unwrap_or_default();

    client
        .get(&format!(
            "{api_address}notifications?{status_parameter}{snoozed_notifications_parameter}{task_id_parameter}{notification_kind_parameter}"
        ))
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn list_notifications_with_tasks(
    client: &Client,
    api_address: &str,
    status_filter: Vec<NotificationStatus>,
    include_snoozed_notifications: bool,
    task_id: Option<TaskId>,
    notification_kind: Option<NotificationSourceKind>,
) -> Vec<NotificationWithTask> {
    let notifications_page: Page<NotificationWithTask> = list_notifications_response(
        client,
        api_address,
        status_filter,
        include_snoozed_notifications,
        task_id,
        notification_kind,
    )
    .await
    .json()
    .await
    .expect("Cannot parse JSON result");

    notifications_page.content
}

pub async fn list_notifications(
    client: &Client,
    api_address: &str,
    status_filter: Vec<NotificationStatus>,
    include_snoozed_notifications: bool,
    task_id: Option<TaskId>,
    notification_kind: Option<NotificationSourceKind>,
) -> Vec<Notification> {
    list_notifications_with_tasks(
        client,
        api_address,
        status_filter,
        include_snoozed_notifications,
        task_id,
        notification_kind,
    )
    .await
    .into_iter()
    .map(|n| n.into())
    .collect()
}

pub async fn sync_notifications_response(
    client: &Client,
    api_address: &str,
    source: Option<NotificationSourceKind>,
    asynchronous: bool,
) -> Response {
    client
        .post(&format!("{api_address}notifications/sync"))
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
    api_address: &str,
    source: Option<NotificationSourceKind>,
    asynchronous: bool,
) -> Vec<Notification> {
    sync_notifications_response(client, api_address, source, asynchronous)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}

pub async fn create_task_from_notification_response(
    client: &Client,
    api_address: &str,
    notification_id: NotificationId,
    task_creation: &TaskCreation,
) -> Response {
    client
        .post(&format!(
            "{api_address}notifications/{notification_id}/task"
        ))
        .json(task_creation)
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn create_task_from_notification(
    client: &Client,
    api_address: &str,
    notification_id: NotificationId,
    task_creation: &TaskCreation,
) -> Option<NotificationWithTask> {
    create_task_from_notification_response(client, api_address, notification_id, task_creation)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}

pub async fn create_or_update_notification_details(
    app: &AuthenticatedApp,
    notification_id: NotificationId,
    details: NotificationDetails,
) -> NotificationDetails {
    let mut transaction = app.app.repository.begin().await.unwrap();
    let upsert_status = app
        .app
        .repository
        .create_or_update_notification_details(&mut transaction, notification_id, details)
        .await
        .unwrap();

    transaction.commit().await.unwrap();

    upsert_status.value()
}
