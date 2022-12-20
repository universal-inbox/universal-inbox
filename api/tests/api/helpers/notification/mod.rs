#![allow(clippy::useless_conversion)]

use reqwest::Response;
use serde_json::json;
use uuid::Uuid;

use universal_inbox::notification::{Notification, NotificationStatus};

use universal_inbox_api::integrations::notification::NotificationSourceKind;

pub mod github;

pub async fn list_notifications_response(
    app_address: &str,
    status_filter: NotificationStatus,
    include_snoozed_notifications: bool,
    task_id: Option<Uuid>,
) -> Response {
    let snoozed_notifications_parameter = if include_snoozed_notifications {
        "&include_snoozed_notifications=true"
    } else {
        ""
    };
    let task_id_parameter = task_id
        .map(|id| format!("&task_id={id}"))
        .unwrap_or_default();

    reqwest::Client::new()
        .get(&format!(
            "{app_address}/notifications?status={status_filter}{snoozed_notifications_parameter}{task_id_parameter}"
        ))
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn list_notifications(
    app_address: &str,
    status_filter: NotificationStatus,
    include_snoozed_notifications: bool,
    task_id: Option<Uuid>,
) -> Vec<Notification> {
    list_notifications_response(
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
