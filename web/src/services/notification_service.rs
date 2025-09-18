use anyhow::Result;
use chrono::{DateTime, Local, TimeDelta, TimeZone, Timelike, Utc};
use dioxus::prelude::*;
use futures_util::StreamExt;
use reqwest::Method;
use strum::IntoEnumIterator;
use url::Url;

use universal_inbox::{
    notification::{
        service::{
            InvitationPatch, NotificationPatch, PatchNotificationsRequest,
            SyncNotificationsParameters,
        },
        Notification, NotificationId, NotificationListOrder, NotificationSourceKind,
        NotificationStatus, NotificationSyncSourceKind, NotificationWithTask,
    },
    task::{TaskCreation, TaskId, TaskPlanning},
    third_party::integrations::google_calendar::GoogleCalendarEventAttendeeResponseStatus,
    Page, PageToken,
};

use crate::{
    model::UniversalInboxUIModel,
    services::{
        api::{call_api, call_api_and_notify},
        task_service::TaskCommand,
        toast_service::ToastCommand,
    },
};

#[derive(Debug, PartialEq)]
pub enum NotificationCommand {
    Refresh,
    Sync(Option<NotificationSyncSourceKind>),
    DeleteFromNotification(NotificationWithTask),
    DeleteAll,
    Unsubscribe(NotificationId),
    Snooze(NotificationId),
    CompleteTaskFromNotification(NotificationWithTask),
    PlanTask(NotificationWithTask, TaskId, TaskPlanning),
    CreateTaskWithDetaultsFromNotification(NotificationWithTask),
    CreateTaskFromNotification(NotificationWithTask, TaskCreation),
    LinkNotificationWithTask(NotificationId, TaskId),
    AcceptInvitation(NotificationId),
    DeclineInvitation(NotificationId),
    TentativelyAcceptInvitation(NotificationId),
}

pub static NOTIFICATIONS_PAGE: GlobalSignal<Page<NotificationWithTask>> =
    Signal::global(Page::default);
pub static NOTIFICATION_FILTERS: GlobalSignal<NotificationFilters> =
    Signal::global(NotificationFilters::default);

pub async fn notification_service(
    mut rx: UnboundedReceiver<NotificationCommand>,
    api_base_url: Url,
    mut notifications_page: Signal<Page<NotificationWithTask>>,
    notification_filters: Signal<NotificationFilters>,
    ui_model: Signal<UniversalInboxUIModel>,
    task_service: Coroutine<TaskCommand>,
    toast_service: Coroutine<ToastCommand>,
) {
    loop {
        let msg = rx.next().await;
        match msg {
            Some(NotificationCommand::Refresh) => {
                refresh_notifications(
                    &api_base_url,
                    notifications_page,
                    notification_filters,
                    ui_model,
                )
                .await;
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
                    Some(ui_model),
                    &toast_service,
                    "Syncing notifications...",
                    "Successfully synced notifications",
                )
                .await;
                if result.is_ok() {
                    refresh_notifications(
                        &api_base_url,
                        notifications_page,
                        notification_filters,
                        ui_model,
                    )
                    .await;
                }
            }
            Some(NotificationCommand::DeleteFromNotification(ref notification)) => {
                if let Some(ref task) = notification.task {
                    if notification.is_built_from_task() {
                        delete_task(notification.id, task.id, notifications_page, &task_service)
                            .await;
                    } else {
                        delete_notification(
                            &api_base_url,
                            notification.id,
                            notifications_page,
                            ui_model,
                            &toast_service,
                        )
                        .await;
                    }
                } else {
                    delete_notification(
                        &api_base_url,
                        notification.id,
                        notifications_page,
                        ui_model,
                        &toast_service,
                    )
                    .await;
                }
            }
            Some(NotificationCommand::DeleteAll) => {
                delete_all_notifications(
                    &api_base_url,
                    notifications_page,
                    notification_filters,
                    ui_model,
                    &toast_service,
                )
                .await;
            }
            Some(NotificationCommand::Unsubscribe(notification_id)) => {
                notifications_page
                    .write()
                    .remove_element(|notif| notif.id != notification_id);

                let _result: Result<Option<Notification>> = call_api_and_notify(
                    Method::PATCH,
                    &api_base_url,
                    &format!("notifications/{notification_id}"),
                    Some(NotificationPatch {
                        status: Some(NotificationStatus::Unsubscribed),
                        ..Default::default()
                    }),
                    Some(ui_model),
                    &toast_service,
                    "Unsubscribing from notification...",
                    "Successfully unsubscribed from notification",
                )
                .await;
            }
            Some(NotificationCommand::Snooze(notification_id)) => {
                let snoozed_time = compute_snoozed_until(Local::now(), 1, 6);

                notifications_page
                    .write()
                    .remove_element(|notif| notif.id != notification_id);

                let _result: Result<Option<Notification>> = call_api_and_notify(
                    Method::PATCH,
                    &api_base_url,
                    &format!("notifications/{notification_id}"),
                    Some(NotificationPatch {
                        snoozed_until: Some(snoozed_time),
                        ..Default::default()
                    }),
                    Some(ui_model),
                    &toast_service,
                    "Snoozing notification...",
                    "Successfully snoozed notification",
                )
                .await;
            }
            Some(NotificationCommand::CompleteTaskFromNotification(ref notification)) => {
                if let Some(ref task) = notification.task {
                    if notification.is_built_from_task() {
                        notifications_page
                            .write()
                            .remove_element(|notif| notif.id != notification.id);

                        task_service.send(TaskCommand::Complete(task.id));
                    }
                }
            }
            Some(NotificationCommand::PlanTask(ref notification, task_id, parameters)) => {
                notifications_page
                    .write()
                    .remove_element(|notif| notif.id != notification.id);

                task_service.send(TaskCommand::Plan(task_id, parameters));
            }
            Some(NotificationCommand::CreateTaskWithDetaultsFromNotification(ref notification)) => {
                notifications_page
                    .write()
                    .remove_element(|notif| notif.id != notification.id);

                let _result: Result<Option<NotificationWithTask>> = call_api_and_notify(
                    Method::POST,
                    &api_base_url,
                    &format!("notifications/{}/task", notification.id),
                    None::<TaskCreation>,
                    Some(ui_model),
                    &toast_service,
                    "Creating task with default settings from notification...",
                    "Task successfully created",
                )
                .await;
            }
            Some(NotificationCommand::CreateTaskFromNotification(ref notification, parameters)) => {
                notifications_page
                    .write()
                    .remove_element(|notif| notif.id != notification.id);

                let _result: Result<Option<NotificationWithTask>> = call_api_and_notify(
                    Method::POST,
                    &api_base_url,
                    &format!("notifications/{}/task", notification.id),
                    Some(parameters),
                    Some(ui_model),
                    &toast_service,
                    "Creating task from notification...",
                    "Task successfully created",
                )
                .await;
            }
            Some(NotificationCommand::LinkNotificationWithTask(notification_id, task_id)) => {
                notifications_page
                    .write()
                    .remove_element(|notif| notif.id != notification_id);

                let _result: Result<Option<NotificationWithTask>> = call_api_and_notify(
                    Method::PATCH,
                    &api_base_url,
                    &format!("notifications/{notification_id}"),
                    Some(NotificationPatch {
                        status: Some(NotificationStatus::Deleted),
                        task_id: Some(task_id),
                        ..Default::default()
                    }),
                    Some(ui_model),
                    &toast_service,
                    "Linking notification...",
                    "Notification successfully linked",
                )
                .await;
            }
            Some(NotificationCommand::AcceptInvitation(notification_id)) => {
                patch_invitation(
                    notifications_page,
                    notification_id,
                    &api_base_url,
                    GoogleCalendarEventAttendeeResponseStatus::Accepted,
                    ui_model,
                    toast_service,
                )
                .await;
            }
            Some(NotificationCommand::DeclineInvitation(notification_id)) => {
                patch_invitation(
                    notifications_page,
                    notification_id,
                    &api_base_url,
                    GoogleCalendarEventAttendeeResponseStatus::Declined,
                    ui_model,
                    toast_service,
                )
                .await;
            }
            Some(NotificationCommand::TentativelyAcceptInvitation(notification_id)) => {
                patch_invitation(
                    notifications_page,
                    notification_id,
                    &api_base_url,
                    GoogleCalendarEventAttendeeResponseStatus::Tentative,
                    ui_model,
                    toast_service,
                )
                .await;
            }
            None => {}
        }
    }
}

async fn refresh_notifications(
    api_base_url: &Url,
    mut notifications_page: Signal<Page<NotificationWithTask>>,
    notification_filters: Signal<NotificationFilters>,
    ui_model: Signal<UniversalInboxUIModel>,
) {
    let source_filters: Vec<String> = notification_filters()
        .notification_source_kind_filters
        .iter()
        .filter(|f| f.selected)
        .map(|f| f.kind.to_string())
        .collect();

    let mut parameters = vec![
        ("status", "Unread,Read".to_string()),
        ("with_tasks", "true".to_string()),
        ("order_by", notification_filters().sort_by.to_string()),
        ("sources", source_filters.join(",")),
    ];
    if let Ok(url_parameters) = notification_filters().current_page_token.to_url_parameter() {
        parameters.push(("page_token", url_parameters));
    }

    let filters = serde_urlencoded::to_string(parameters).unwrap_or_default();
    let result: Result<Page<NotificationWithTask>> = call_api(
        Method::GET,
        api_base_url,
        &format!("notifications?{filters}"),
        // random type as we don't care about the body's type
        None::<i32>,
        Some(ui_model),
    )
    .await;

    if let Ok(new_notifications_page) = result {
        *notifications_page.write() = new_notifications_page;
    }
}

async fn delete_notification(
    api_base_url: &Url,
    notification_id: NotificationId,
    mut notifications_page: Signal<Page<NotificationWithTask>>,
    ui_model: Signal<UniversalInboxUIModel>,
    toast_service: &Coroutine<ToastCommand>,
) {
    notifications_page
        .write()
        .remove_element(|notif| notif.id != notification_id);

    let _result: Result<Option<Notification>, anyhow::Error> = call_api_and_notify(
        Method::PATCH,
        api_base_url,
        &format!("notifications/{notification_id}"),
        Some(NotificationPatch {
            status: Some(NotificationStatus::Deleted),
            ..Default::default()
        }),
        Some(ui_model),
        toast_service,
        "Deleting notification...",
        "Successfully deleted notification",
    )
    .await;
}

async fn delete_task(
    notification_id: NotificationId,
    task_id: TaskId,
    mut notifications_page: Signal<Page<NotificationWithTask>>,
    task_service: &Coroutine<TaskCommand>,
) {
    notifications_page
        .write()
        .remove_element(|notif| notif.id != notification_id);

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
        from + TimeDelta::try_days(days_offset).unwrap()
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

async fn delete_all_notifications(
    api_base_url: &Url,
    mut notifications_page: Signal<Page<NotificationWithTask>>,
    notification_filters: Signal<NotificationFilters>,
    ui_model: Signal<UniversalInboxUIModel>,
    toast_service: &Coroutine<ToastCommand>,
) {
    // Get current filter parameters to match what's currently visible
    let source_filters = notification_filters().selected();

    let patch_request = PatchNotificationsRequest {
        status: vec![NotificationStatus::Unread, NotificationStatus::Read],
        sources: source_filters,
        patch: NotificationPatch {
            status: Some(NotificationStatus::Deleted),
            snoozed_until: None,
            task_id: None,
        },
    };

    // Clear the UI immediately for better UX
    notifications_page.write().content.clear();

    let result: Result<serde_json::Value> = call_api_and_notify(
        Method::PATCH,
        api_base_url,
        "notifications",
        Some(patch_request),
        Some(ui_model),
        toast_service,
        "Deleting all notifications...",
        "All notifications have been deleted",
    )
    .await;

    if result.is_ok() {
        // Refresh to make sure UI is in sync
        refresh_notifications(
            api_base_url,
            notifications_page,
            notification_filters,
            ui_model,
        )
        .await;
    }
}

async fn patch_invitation(
    mut notifications_page: Signal<Page<NotificationWithTask>>,
    notification_id: NotificationId,
    api_base_url: &Url,
    response_status: GoogleCalendarEventAttendeeResponseStatus,
    ui_model: Signal<UniversalInboxUIModel>,
    toast_service: Coroutine<ToastCommand>,
) {
    let mut page = notifications_page.write();
    if page.content.iter().any(|notif| {
        notif.id == notification_id && notif.kind == NotificationSourceKind::GoogleCalendar
    }) {
        page.remove_element(|notif| notif.id != notification_id);
        let _result: Result<Option<Notification>> = call_api_and_notify(
            Method::PATCH,
            api_base_url,
            &format!("notifications/{notification_id}/invitation"),
            Some(InvitationPatch { response_status }),
            Some(ui_model),
            &toast_service,
            "Accepting invitation...",
            "Invitation successfully accepted",
        )
        .await;
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct NotificationSourceKindFilter {
    pub selected: bool,
    pub kind: NotificationSourceKind,
}

#[derive(Clone, PartialEq, Debug)]
pub struct NotificationFilters {
    pub notification_source_kind_filters: Vec<NotificationSourceKindFilter>,
    pub sort_by: NotificationListOrder,
    pub current_page_token: PageToken,
}

impl NotificationFilters {
    pub fn select(&mut self, filter: NotificationSourceKindFilter) {
        let selected_count = self
            .notification_source_kind_filters
            .iter()
            .filter(|f| f.selected)
            .count();
        let total = self.notification_source_kind_filters.len();

        if selected_count == total {
            for f in &mut self.notification_source_kind_filters {
                if f.kind != filter.kind {
                    f.selected = false;
                }
            }
        } else if selected_count == 1
            && self
                .notification_source_kind_filters
                .iter()
                .any(|f| f.selected && f.kind == filter.kind)
        {
            for f in &mut self.notification_source_kind_filters {
                f.selected = true;
            }
        } else if let Some(f) = self
            .notification_source_kind_filters
            .iter_mut()
            .find(|f| f.kind == filter.kind)
        {
            f.selected = !f.selected;
        }
    }

    pub fn is_filtered(&self) -> bool {
        self.notification_source_kind_filters
            .iter()
            .any(|f| !f.selected)
    }

    pub fn selected(&self) -> Vec<NotificationSourceKind> {
        self.notification_source_kind_filters
            .iter()
            .filter(|f| f.selected)
            .map(|f| f.kind)
            .collect::<Vec<_>>()
    }
}

impl Default for NotificationFilters {
    fn default() -> Self {
        let mut notification_source_kind_filters: Vec<NotificationSourceKindFilter> =
            NotificationSourceKind::iter()
                .map(|kind| NotificationSourceKindFilter {
                    selected: true,
                    kind,
                })
                .collect();
        notification_source_kind_filters
            .sort_by(|a, b| a.kind.to_string().cmp(&b.kind.to_string()));

        Self {
            notification_source_kind_filters,
            sort_by: NotificationListOrder::default(),
            current_page_token: PageToken::Offset(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;
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
