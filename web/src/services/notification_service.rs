use crate::{
    components::toast_zone::{Toast, ToastKind},
    services::{
        api::{call_api, call_api_with_body},
        toast_service::ToastUpdate,
    },
};
use chrono::{DateTime, Duration, Local, TimeZone, Timelike, Utc};
use dioxus::{fermi::UseAtomRef, prelude::*};
use futures_util::StreamExt;
use std::collections::HashMap;
use universal_inbox::{Notification, NotificationPatch, NotificationStatus};
use wasm_bindgen::JsValue;

use super::toast_service::ToastCommand;

#[derive(Debug)]
pub enum NotificationCommand {
    Refresh,
    Delete(Notification),
    Unsubscribe(Notification),
    Snooze(Notification),
    MarkAsDone(Notification),
}

#[derive(Debug, Default)]
pub struct UniversalInboxUIModel {
    pub selected_notification_index: usize,
    pub footer_help_opened: bool,
}

pub static NOTIFICATIONS: AtomRef<Vec<Notification>> = |_| vec![];
pub static UI_MODEL: AtomRef<UniversalInboxUIModel> = |_| Default::default();

pub async fn notification_service<'a>(
    mut rx: UnboundedReceiver<NotificationCommand>,
    notifications: UseAtomRef<Vec<Notification>>,
    toast_service: CoroutineHandle<ToastCommand>,
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

                let result: Vec<Notification> =
                    call_api("GET", "/notifications?status=Unread", HashMap::new())
                        .await
                        .unwrap();
                notifications.write().extend(result);

                let toast_update = ToastUpdate {
                    id: toast_id,
                    kind: Some(ToastKind::Success),
                    message: Some("Successfully loaded notifications".to_string()),
                    timeout: Some(Some(5_000)),
                };
                toast_service.send(ToastCommand::Update(toast_update));
            }
            Some(NotificationCommand::Delete(notification)) => {
                notifications
                    .write()
                    .retain(|notif| notif.id != notification.id);

                let _result: Notification = call_api_and_notify(
                    "PATCH",
                    &format!("/notifications/{}", notification.id),
                    NotificationPatch {
                        status: Some(NotificationStatus::Deleted),
                        ..Default::default()
                    },
                    HashMap::new(),
                    &toast_service,
                    "Deleting notification...",
                    "Successfully deleted notification",
                )
                .await
                .unwrap();
            }
            Some(NotificationCommand::Unsubscribe(notification)) => {
                notifications
                    .write()
                    .retain(|notif| notif.id != notification.id);

                let _result: Notification = call_api_and_notify(
                    "PATCH",
                    &format!("/notifications/{}", notification.id),
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
            Some(NotificationCommand::Snooze(notification)) => {
                let snoozed_time = compute_snoozed_until(Local::now(), 1, 6);

                notifications
                    .write()
                    .retain(|notif| notif.id != notification.id);

                let _result: Notification = call_api_and_notify(
                    "PATCH",
                    &format!("/notifications/{}", notification.id),
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
            Some(NotificationCommand::MarkAsDone(_)) => {}
            None => {}
        }
    }
}

async fn call_api_and_notify<R: for<'de> serde::de::Deserialize<'de>, B: serde::Serialize>(
    method: &str,
    path: &str,
    body: B,
    headers: HashMap<String, String>,
    toast_service: &CoroutineHandle<ToastCommand>,
    loading_message: &str,
    success_message: &str,
) -> Result<R, JsValue> {
    let toast = Toast {
        kind: ToastKind::Loading,
        message: loading_message.to_string(),
        ..Default::default()
    };
    let toast_id = toast.id;
    toast_service.send(ToastCommand::Push(toast));

    let result: R = call_api_with_body(method, path, body, headers).await?;

    let toast_update = ToastUpdate {
        id: toast_id,
        kind: Some(ToastKind::Success),
        message: Some(success_message.to_string()),
        timeout: Some(Some(5_000)),
    };
    toast_service.send(ToastCommand::Update(toast_update));

    Ok(result)
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
