use anyhow::Result;
use chrono::{DateTime, Duration, Local, TimeZone, Timelike, Utc};
use dioxus::prelude::*;
use fermi::{AtomRef, UseAtomRef};
use futures_util::StreamExt;
use reqwest::Method;
use url::Url;

use universal_inbox::{
    notification::{
        service::{NotificationPatch, SyncNotificationsParameters},
        Notification, NotificationId, NotificationStatus, NotificationSyncSourceKind,
        NotificationWithTask,
    },
    task::{TaskCreation, TaskId, TaskPlanning},
};

use crate::{
    model::UniversalInboxUIModel,
    services::{
        api::{call_api, call_api_and_notify},
        task_service::TaskCommand,
        toast_service::ToastCommand,
    },
};

#[derive(Debug)]
pub enum NotificationCommand {
    Refresh,
    Sync(Option<NotificationSyncSourceKind>),
    DeleteFromNotification(NotificationWithTask),
    Unsubscribe(NotificationId),
    Snooze(NotificationId),
    CompleteTaskFromNotification(NotificationWithTask),
    PlanTask(NotificationWithTask, TaskId, TaskPlanning),
    CreateTaskFromNotification(NotificationWithTask, TaskCreation),
    LinkNotificationWithTask(NotificationId, TaskId),
}

pub static NOTIFICATIONS: AtomRef<Vec<NotificationWithTask>> = |_| vec![];

pub async fn notification_service<'a>(
    mut rx: UnboundedReceiver<NotificationCommand>,
    api_base_url: Url,
    notifications: UseAtomRef<Vec<NotificationWithTask>>,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
    task_service: Coroutine<TaskCommand>,
    toast_service: Coroutine<ToastCommand>,
) {
    loop {
        let msg = rx.next().await;
        match msg {
            Some(NotificationCommand::Refresh) => {
                refresh_notifications(&api_base_url, &notifications, &ui_model_ref).await;
            }
            Some(NotificationCommand::Sync(source)) => {
                let result: Result<Vec<Notification>> = call_api_and_notify(
                    Method::POST,
                    &api_base_url,
                    "notifications/sync",
                    Some(SyncNotificationsParameters {
                        source,
                        asynchronous: Some(false),
                    }),
                    Some(ui_model_ref.clone()),
                    &toast_service,
                    "Syncing notifications...",
                    "Successfully synced notifications",
                )
                .await;
                if result.is_ok() {
                    refresh_notifications(&api_base_url, &notifications, &ui_model_ref).await;
                }
            }
            Some(NotificationCommand::DeleteFromNotification(notification)) => {
                if let Some(ref task) = notification.task {
                    if notification.is_built_from_task() {
                        delete_task(notification.id, task.id, &notifications, &task_service).await;
                    } else {
                        delete_notification(
                            &api_base_url,
                            notification.id,
                            &notifications,
                            &ui_model_ref,
                            &toast_service,
                        )
                        .await;
                    }
                } else {
                    delete_notification(
                        &api_base_url,
                        notification.id,
                        &notifications,
                        &ui_model_ref,
                        &toast_service,
                    )
                    .await;
                }
            }
            Some(NotificationCommand::Unsubscribe(notification_id)) => {
                notifications
                    .write()
                    .retain(|notif| notif.id != notification_id);

                let _result: Result<Notification> = call_api_and_notify(
                    Method::PATCH,
                    &api_base_url,
                    &format!("notifications/{notification_id}"),
                    Some(NotificationPatch {
                        status: Some(NotificationStatus::Unsubscribed),
                        ..Default::default()
                    }),
                    Some(ui_model_ref.clone()),
                    &toast_service,
                    "Unsubscribing from notification...",
                    "Successfully unsubscribed from notification",
                )
                .await;
            }
            Some(NotificationCommand::Snooze(notification_id)) => {
                let snoozed_time = compute_snoozed_until(Local::now(), 1, 6);

                notifications
                    .write()
                    .retain(|notif| notif.id != notification_id);

                let _result: Result<Notification> = call_api_and_notify(
                    Method::PATCH,
                    &api_base_url,
                    &format!("notifications/{notification_id}"),
                    Some(NotificationPatch {
                        snoozed_until: Some(snoozed_time),
                        ..Default::default()
                    }),
                    Some(ui_model_ref.clone()),
                    &toast_service,
                    "Snoozing notification...",
                    "Successfully snoozed notification",
                )
                .await;
            }
            Some(NotificationCommand::CompleteTaskFromNotification(notification)) => {
                if let Some(ref task) = notification.task {
                    if notification.is_built_from_task() {
                        notifications
                            .write()
                            .retain(|notif| notif.id != notification.id);

                        task_service.send(TaskCommand::Complete(task.id));
                    }
                }
            }
            Some(NotificationCommand::PlanTask(notification, task_id, parameters)) => {
                notifications
                    .write()
                    .retain(|notif| notif.id != notification.id);

                task_service.send(TaskCommand::Plan(task_id, parameters));
            }
            Some(NotificationCommand::CreateTaskFromNotification(notification, parameters)) => {
                notifications
                    .write()
                    .retain(|notif| notif.id != notification.id);

                let _result: Result<Option<NotificationWithTask>> = call_api_and_notify(
                    Method::POST,
                    &api_base_url,
                    &format!("notifications/{}/task", notification.id),
                    Some(parameters),
                    Some(ui_model_ref.clone()),
                    &toast_service,
                    "Creating task from notification...",
                    "Task successfully created",
                )
                .await;
            }
            Some(NotificationCommand::LinkNotificationWithTask(notification_id, task_id)) => {
                notifications
                    .write()
                    .retain(|notif| notif.id != notification_id);

                let _result: Result<NotificationWithTask> = call_api_and_notify(
                    Method::PATCH,
                    &api_base_url,
                    &format!("notifications/{notification_id}"),
                    Some(NotificationPatch {
                        status: Some(NotificationStatus::Deleted),
                        task_id: Some(task_id),
                        ..Default::default()
                    }),
                    Some(ui_model_ref.clone()),
                    &toast_service,
                    "Linking notification...",
                    "Notification successfully linked",
                )
                .await;
            }
            None => {}
        }
    }
}

async fn refresh_notifications(
    api_base_url: &Url,
    notifications: &UseAtomRef<Vec<NotificationWithTask>>,
    ui_model_ref: &UseAtomRef<UniversalInboxUIModel>,
) {
    ui_model_ref.write().loaded_notifications = None;

    let result: Result<Vec<NotificationWithTask>> = call_api(
        Method::GET,
        api_base_url,
        "notifications?status=Unread&with_tasks=true",
        // random type as we don't care about the body's type
        None::<i32>,
        Some(ui_model_ref.clone()),
    )
    .await;

    match result {
        Ok(new_notifications) => {
            let mut notifications = notifications.write();
            notifications.clear();
            notifications.extend(new_notifications);
            ui_model_ref.write().loaded_notifications = Some(Ok(notifications.len()));
        }
        Err(err) => {
            ui_model_ref.write().loaded_notifications =
                Some(Err(format!("Failed to load notifications: {err}")));
        }
    }
}

async fn delete_notification(
    api_base_url: &Url,
    notification_id: NotificationId,
    notifications: &UseAtomRef<Vec<NotificationWithTask>>,
    ui_model_ref: &UseAtomRef<UniversalInboxUIModel>,
    toast_service: &Coroutine<ToastCommand>,
) {
    notifications
        .write()
        .retain(|notif| notif.id != notification_id);

    let _result: Result<Notification, anyhow::Error> = call_api_and_notify(
        Method::PATCH,
        api_base_url,
        &format!("notifications/{notification_id}"),
        Some(NotificationPatch {
            status: Some(NotificationStatus::Deleted),
            ..Default::default()
        }),
        Some(ui_model_ref.clone()),
        toast_service,
        "Deleting notification...",
        "Successfully deleted notification",
    )
    .await;
}

async fn delete_task(
    notification_id: NotificationId,
    task_id: TaskId,
    notifications: &UseAtomRef<Vec<NotificationWithTask>>,
    task_service: &Coroutine<TaskCommand>,
) {
    notifications
        .write()
        .retain(|notif| notif.id != notification_id);

    task_service.send(TaskCommand::Delete(task_id));
}

fn compute_snoozed_until<Tz: TimeZone>(
    from: DateTime<Tz>,
    days_offset: i64,
    reset_hour: u32,
) -> DateTime<Utc>
where
    DateTime<Utc>: From<DateTime<Tz>>,
{
    let day_adjusted_time = if from.hour() < reset_hour {
        from
    } else {
        from + Duration::days(days_offset)
    };
    day_adjusted_time
        .with_hour(reset_hour)
        .unwrap()
        .with_minute(0)
        .unwrap()
        .with_second(0)
        .unwrap()
        .with_nanosecond(0)
        .unwrap()
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, TimeZone};
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn test_compute_snoozed_until_localized_before_reset_hour_utc_before_reset_hour() {
        assert_eq!(
            compute_snoozed_until(
                FixedOffset::east_opt(5 * 3600)
                    .unwrap()
                    .with_ymd_and_hms(2022, 1, 1, 0, 3, 42)
                    .unwrap(),
                1,
                6
            ),
            Utc.with_ymd_and_hms(2022, 1, 1, 1, 0, 0).unwrap()
        );
    }

    #[wasm_bindgen_test]
    fn test_compute_snoozed_until_localized_before_reset_hour_utc_after_reset_hour() {
        assert_eq!(
            compute_snoozed_until(
                FixedOffset::east_opt(7 * 3600)
                    .unwrap()
                    .with_ymd_and_hms(2022, 1, 1, 0, 3, 42)
                    .unwrap(),
                1,
                6
            ),
            Utc.with_ymd_and_hms(2021, 12, 31, 23, 0, 0).unwrap()
        );
    }

    #[wasm_bindgen_test]
    fn test_compute_snoozed_until_localized_after_reset_hour_utc_after_reset_hour() {
        assert_eq!(
            compute_snoozed_until(
                FixedOffset::east_opt(5 * 3600)
                    .unwrap()
                    .with_ymd_and_hms(2022, 1, 1, 12, 3, 42)
                    .unwrap(),
                1,
                6
            ),
            Utc.with_ymd_and_hms(2022, 1, 2, 1, 0, 0).unwrap()
        );
    }

    #[wasm_bindgen_test]
    fn test_compute_snoozed_until_localized_after_reset_hour_utc_before_reset_hour() {
        assert_eq!(
            compute_snoozed_until(
                FixedOffset::east_opt(7 * 3600)
                    .unwrap()
                    .with_ymd_and_hms(2022, 1, 1, 12, 3, 42)
                    .unwrap(),
                1,
                6
            ),
            Utc.with_ymd_and_hms(2022, 1, 1, 23, 0, 0).unwrap()
        );
    }
}
