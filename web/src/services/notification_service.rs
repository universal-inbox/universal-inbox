use std::collections::HashMap;

use chrono::{DateTime, Duration, Local, TimeZone, Timelike, Utc};
use dioxus::prelude::*;
use fermi::{AtomRef, UseAtomRef};
use futures_util::StreamExt;
use log::{debug, error};

use universal_inbox::{
    notification::{Notification, NotificationId, NotificationPatch, NotificationStatus},
    task::TaskId,
    NotificationsListResult,
};

use crate::{
    components::toast_zone::{Toast, ToastKind},
    services::{
        api::{call_api, call_api_and_notify},
        task_service::{TaskCommand, TaskPlanningParameters},
        toast_service::{ToastCommand, ToastUpdate},
    },
};

#[derive(Debug)]
pub enum NotificationCommand {
    Refresh,
    DeleteFromNotification(Notification),
    Unsubscribe(NotificationId),
    Snooze(NotificationId),
    CompleteTaskFromNotification(Notification),
    PlanTask(Notification, TaskPlanningParameters),
}

#[derive(Debug, Default)]
pub struct UniversalInboxUIModel {
    pub selected_notification_index: usize,
    pub footer_help_opened: bool,
    pub task_planning_modal_opened: bool,
    pub unhover_element: bool,
}

impl UniversalInboxUIModel {
    pub fn toggle_help(&mut self) {
        self.footer_help_opened = !self.footer_help_opened;
    }

    pub fn set_unhover_element(&mut self, unhover_element: bool) -> bool {
        if self.unhover_element != unhover_element {
            self.unhover_element = unhover_element;
            return true;
        }
        false
    }
}

pub static NOTIFICATIONS: AtomRef<Vec<Notification>> = |_| vec![];
pub static UI_MODEL: AtomRef<UniversalInboxUIModel> = |_| Default::default();

pub async fn notification_service<'a>(
    mut rx: UnboundedReceiver<NotificationCommand>,
    notifications: UseAtomRef<Vec<Notification>>,
    task_service: Coroutine<TaskCommand>,
    toast_service: Coroutine<ToastCommand>,
) {
    loop {
        let msg = rx.next().await;
        match msg {
            Some(NotificationCommand::Refresh) => {
                let toast = Toast {
                    kind: ToastKind::Loading,
                    message: "Loading notifications...".to_string(),
                    ..Default::default()
                };
                let toast_id = toast.id;
                toast_service.send(ToastCommand::Push(toast));

                let result: NotificationsListResult = call_api(
                    "GET",
                    "/notifications?status=Unread&with_tasks=true",
                    HashMap::new(),
                )
                .await
                .unwrap();

                debug!("{} notifications loaded", result.notifications.len());
                notifications.write().extend(result.notifications);
                if let Some(tasks) = result.tasks {
                    task_service.send(TaskCommand::UpdateTasks(tasks));
                } else {
                    error!("No tasks in notifications list result");
                }

                let toast_update = ToastUpdate {
                    id: toast_id,
                    kind: Some(ToastKind::Success),
                    message: Some("Successfully loaded notifications".to_string()),
                    timeout: Some(Some(5_000)),
                };
                toast_service.send(ToastCommand::Update(toast_update));
            }
            Some(NotificationCommand::DeleteFromNotification(notification)) => {
                if let Some(task_id) = notification.task_id {
                    if notification.is_built_from_task() {
                        delete_task(notification.id, task_id, &notifications, &task_service).await;
                    } else {
                        delete_notification(notification.id, &notifications, &toast_service).await;
                    }
                } else {
                    delete_notification(notification.id, &notifications, &toast_service).await;
                }
            }
            Some(NotificationCommand::Unsubscribe(notification_id)) => {
                notifications
                    .write()
                    .retain(|notif| notif.id != notification_id);

                let _result: Notification = call_api_and_notify(
                    "PATCH",
                    &format!("/notifications/{}", notification_id),
                    NotificationPatch {
                        status: Some(NotificationStatus::Unsubscribed),
                        ..Default::default()
                    },
                    HashMap::new(),
                    &toast_service,
                    "Unsubscribing from notification...",
                    "Successfully unsubscribed from notification",
                )
                .await
                .unwrap();
            }
            Some(NotificationCommand::Snooze(notification_id)) => {
                let snoozed_time = compute_snoozed_until(Local::now(), 1, 6);

                notifications
                    .write()
                    .retain(|notif| notif.id != notification_id);

                let _result: Notification = call_api_and_notify(
                    "PATCH",
                    &format!("/notifications/{}", notification_id),
                    NotificationPatch {
                        snoozed_until: Some(snoozed_time),
                        ..Default::default()
                    },
                    HashMap::new(),
                    &toast_service,
                    "Snoozing notification...",
                    "Successfully snoozed notification",
                )
                .await
                .unwrap();
            }
            Some(NotificationCommand::CompleteTaskFromNotification(notification)) => {
                if let Some(task_id) = notification.task_id {
                    if notification.is_built_from_task() {
                        notifications
                            .write()
                            .retain(|notif| notif.id != notification.id);

                        task_service.send(TaskCommand::Complete(task_id));
                    }
                }
            }
            Some(NotificationCommand::PlanTask(notification, parameters)) => {
                notifications
                    .write()
                    .retain(|notif| notif.id != notification.id);

                task_service.send(TaskCommand::Plan(parameters));
            }
            None => {}
        }
    }
}

async fn delete_notification(
    notification_id: NotificationId,
    notifications: &UseAtomRef<Vec<Notification>>,
    toast_service: &Coroutine<ToastCommand>,
) {
    notifications
        .write()
        .retain(|notif| notif.id != notification_id);

    let _result: Notification = call_api_and_notify(
        "PATCH",
        &format!("/notifications/{}", notification_id),
        NotificationPatch {
            status: Some(NotificationStatus::Deleted),
            ..Default::default()
        },
        HashMap::new(),
        toast_service,
        "Deleting notification...",
        "Successfully deleted notification",
    )
    .await
    .unwrap();
}

async fn delete_task(
    notification_id: NotificationId,
    task_id: TaskId,
    notifications: &UseAtomRef<Vec<Notification>>,
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
    use rstest::*;

    #[rstest]
    #[case::localized_before_reset_hour_utc_before_reset_hour(5, 0, 2022, 1, 1, 1)]
    #[case::localized_before_reset_hour_utc_after_reset_hour(7, 0, 2021, 12, 31, 23)]
    #[case::localized_after_reset_hour_utc_after_reset_hour(5, 12, 2022, 1, 2, 1)]
    #[case::localized_after_reset_hour_utc_before_reset_hour(7, 12, 2022, 1, 1, 23)]
    fn test_compute_snoozed_until(
        #[case] offset_hour: i32,
        #[case] current_hour: u32,
        #[case] expected_year: i32,
        #[case] expected_month: u32,
        #[case] expected_day: u32,
        #[case] expected_hour: u32,
    ) {
        assert_eq!(
            compute_snoozed_until(
                FixedOffset::east_opt(offset_hour * 3600)
                    .unwrap()
                    .with_ymd_and_hms(2022, 1, 1, current_hour, 3, 42)
                    .unwrap(),
                1,
                6
            ),
            Utc.with_ymd_and_hms(
                expected_year,
                expected_month,
                expected_day,
                expected_hour,
                0,
                0
            )
            .unwrap()
        );
    }
}
