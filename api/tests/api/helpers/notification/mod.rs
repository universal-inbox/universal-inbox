#![allow(clippy::useless_conversion)]

use std::{fmt::Debug, sync::Arc};

use anyhow::anyhow;
use reqwest::{Client, Response};
use serde_json::json;
use tokio_retry::{strategy::FixedInterval, Retry};

use universal_inbox::{
    integration_connection::IntegrationConnectionId,
    notification::{
        service::NotificationPatch, Notification, NotificationId, NotificationSource,
        NotificationSourceKind, NotificationStatus, NotificationWithTask,
    },
    task::{TaskCreation, TaskId},
    third_party::item::{ThirdPartyItem, ThirdPartyItemData},
    user::UserId,
    Page,
};

use universal_inbox_api::{
    integrations::notification::ThirdPartyNotificationSourceService,
    repository::{notification::NotificationRepository, third_party::ThirdPartyItemRepository},
};

use crate::helpers::{auth::AuthenticatedApp, TestedApp};

pub mod github;
pub mod google_calendar;
pub mod google_mail;
pub mod linear;
pub mod slack;
pub mod todoist;

pub async fn list_notifications_response(
    client: &Client,
    api_address: &str,
    status_filter: Vec<NotificationStatus>,
    include_snoozed_notifications: bool,
    task_id: Option<TaskId>,
    notification_kind: Option<NotificationSourceKind>,
    trigger_sync: bool,
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
        .map(|kind| format!("sources={kind}&"))
        .unwrap_or_default();

    client
        .get(format!(
            "{api_address}notifications?trigger_sync={trigger_sync}&{status_parameter}{snoozed_notifications_parameter}{task_id_parameter}{notification_kind_parameter}"
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
    trigger_sync: bool,
) -> Vec<NotificationWithTask> {
    let notifications_page: Page<NotificationWithTask> = list_notifications_response(
        client,
        api_address,
        status_filter,
        include_snoozed_notifications,
        task_id,
        notification_kind,
        trigger_sync,
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
    trigger_sync: bool,
) -> Vec<Notification> {
    list_notifications_with_tasks(
        client,
        api_address,
        status_filter,
        include_snoozed_notifications,
        task_id,
        notification_kind,
        trigger_sync,
    )
    .await
    .into_iter()
    .map(|n| n.into())
    .collect()
}

pub async fn list_notifications_until(
    client: &Client,
    api_address: &str,
    notification_status: Vec<NotificationStatus>,
    expected_notifications_count: usize,
) -> Vec<Notification> {
    Retry::spawn(FixedInterval::from_millis(500).take(10), || async {
        let notifications = list_notifications(
            client,
            api_address,
            notification_status.clone(),
            false,
            None,
            None,
            false,
        )
        .await;

        if notifications.len() == expected_notifications_count {
            Ok(notifications)
        } else {
            Err(anyhow!("Not yet synchronized"))
        }
    })
    .await
    .unwrap()
}

pub async fn sync_notifications_response(
    client: &Client,
    api_address: &str,
    source: Option<NotificationSourceKind>,
    asynchronous: bool,
) -> Response {
    client
        .post(format!("{api_address}notifications/sync"))
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
        .post(format!("{api_address}notifications/{notification_id}/task"))
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

pub async fn create_notification_from_source_item<T, U>(
    app: &TestedApp,
    source_item_id: String,
    third_party_item_data: ThirdPartyItemData,
    third_party_notification_service: Arc<U>,
    user_id: UserId,
    integration_connection_id: IntegrationConnectionId,
) -> Box<Notification>
where
    T: TryFrom<ThirdPartyItem> + Debug,
    U: ThirdPartyNotificationSourceService<T> + NotificationSource + Send + Sync,
    <T as TryFrom<ThirdPartyItem>>::Error: Send + Sync,
{
    let mut transaction = app.repository.begin().await.unwrap();
    let third_party_item = ThirdPartyItem::new(
        source_item_id,
        third_party_item_data,
        user_id,
        integration_connection_id,
    );
    let third_party_item = app
        .repository
        .create_or_update_third_party_item(&mut transaction, Box::new(third_party_item))
        .await
        .unwrap()
        .value();

    let notification = app
        .notification_service
        .read()
        .await
        .create_notification_from_third_party_item(
            &mut transaction,
            *third_party_item,
            third_party_notification_service,
            user_id,
        )
        .await
        .unwrap()
        .unwrap();

    transaction.commit().await.unwrap();

    Box::new(notification)
}

pub async fn update_notification(
    app: &AuthenticatedApp,
    notification_id: NotificationId,
    patch: &NotificationPatch,
    user_id: UserId,
) -> Box<Notification> {
    let mut transaction = app.app.repository.begin().await.unwrap();
    let update_status = app
        .app
        .repository
        .update_notification(&mut transaction, notification_id, patch, user_id)
        .await
        .unwrap();

    transaction.commit().await.unwrap();

    update_status.result.unwrap()
}
