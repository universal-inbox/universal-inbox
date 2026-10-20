use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Local, TimeDelta, TimeZone, Timelike, Utc};
use dioxus::prelude::*;
use futures_util::StreamExt;
use js_sys::Date;
use reqwest::Method;
use strum::IntoEnumIterator;
use url::Url;

use universal_inbox::{
    Page, PageToken,
    notification::{
        Notification, NotificationId, NotificationListOrder, NotificationSourceKind,
        NotificationStatus, NotificationSyncSourceKind, NotificationWithTask,
        service::{
            InvitationPatch, NotificationPatch, PatchNotificationsRequest,
            SyncNotificationsParameters,
        },
    },
    task::{TaskCreation, TaskId, TaskPlanning},
    third_party::{
        integrations::google_calendar::{EventMethod, GoogleCalendarEventAttendeeResponseStatus},
        item::ThirdPartyItemData,
    },
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
    /// Refetch the displayed list and the sidebar badge counts
    Refresh,
    /// Refetch the displayed list only, when the counts cannot have changed (another
    /// section, filter, page or order)
    RefreshList,
    /// A realtime event says notifications changed: `ids` of the changed ones, `None` when
    /// unknown. Refreshes like `Refresh` unless the change is this tab's own.
    RefreshFromEvent(Option<Vec<NotificationId>>),
    Sync(Option<NotificationSyncSourceKind>),
    DeleteFromNotification(NotificationWithTask),
    DeleteAll,
    Unsubscribe(NotificationId),
    Snooze(NotificationId),
    Unsnooze(NotificationId),
    Undelete(NotificationId),
    MarkAsRead(NotificationId),
    /// Resolve a notification by id: select it in the current list, or fetch it,
    /// switch to its section and prepend it when it is off the first page.
    LoadAndSelect(NotificationId),
    CompleteTaskFromNotification(NotificationWithTask),
    PlanTask(NotificationWithTask, TaskId, TaskPlanning),
    CreateTaskWithDetaultsFromNotification(NotificationWithTask),
    CreateTaskFromNotification(NotificationWithTask, TaskCreation),
    LinkNotificationWithTask(NotificationId, TaskId),
    AcceptInvitation(NotificationId),
    DeclineInvitation(NotificationId),
    TentativelyAcceptInvitation(NotificationId),
}

/// The list of notifications a section displays: the inbox, snoozed notifications
/// (still `Unread`/`Read` but with `snoozed_until` in the future) or deleted ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotificationSection {
    #[default]
    Inbox,
    Snoozed,
    Deleted,
}

impl NotificationSection {
    pub fn title(&self) -> &'static str {
        match self {
            NotificationSection::Inbox => "Inbox",
            NotificationSection::Snoozed => "Snoozed",
            NotificationSection::Deleted => "Deleted",
        }
    }
}

/// Derive the section a notification belongs to from its status and snooze state.
/// `Unsubscribed` notifications are not listed in any section and fall back to the
/// inbox so a deep-linked one is still shown.
pub fn section_of(notification: &NotificationWithTask) -> NotificationSection {
    match notification.status {
        NotificationStatus::Deleted => NotificationSection::Deleted,
        NotificationStatus::Unread | NotificationStatus::Read
            if notification
                .snoozed_until
                .map(|snoozed_until| snoozed_until > Utc::now())
                .unwrap_or(false) =>
        {
            NotificationSection::Snoozed
        }
        _ => NotificationSection::Inbox,
    }
}

pub static NOTIFICATIONS_PAGE: GlobalSignal<Page<NotificationWithTask>> =
    Signal::global(Page::default);
pub static NOTIFICATION_FILTERS: GlobalSignal<NotificationFilters> =
    Signal::global(NotificationFilters::default);
/// The section currently displayed by the notifications page. Drives both which list
/// `refresh_notifications` fetches and which actions the preview/keyboard expose.
pub static CURRENT_NOTIFICATION_SECTION: GlobalSignal<NotificationSection> =
    Signal::global(NotificationSection::default);
/// Sidebar badge counts, kept independent from `NOTIFICATIONS_PAGE` so they stay
/// correct regardless of which section is currently displayed.
pub static INBOX_COUNT: GlobalSignal<usize> = Signal::global(|| 0);
pub static SNOOZED_COUNT: GlobalSignal<usize> = Signal::global(|| 0);

/// How long the realtime event of a change made by this tab can take to come back.
const LOCAL_CHANGE_TTL_MS: f64 = 10_000.0;
/// Above this many changed notifications, a realtime event carries no ids (the API
/// `realtime::scope::MAX_EVENT_IDS`).
const MAX_EVENT_IDS: usize = 50;

/// The changes this tab made recently: the API echoes their realtime events back to it, and
/// the list is already up to date. Times are in milliseconds since the epoch.
#[derive(Debug, Default)]
struct LocalChanges {
    /// Expiry time of each recently changed notification
    notifications: HashMap<NotificationId, f64>,
    /// Expiry time of a change of too many notifications for its event to carry their ids
    bulk_change: Option<f64>,
}

impl LocalChanges {
    fn prune(&mut self, now: f64) {
        self.notifications.retain(|_, expires_at| *expires_at > now);
        self.bulk_change = self.bulk_change.filter(|expires_at| *expires_at > now);
    }

    /// Record the notifications this tab just changed on the server.
    fn record(&mut self, ids: &[NotificationId], now: f64) {
        self.prune(now);
        for id in ids {
            self.notifications.insert(*id, now + LOCAL_CHANGE_TTL_MS);
        }
        if ids.len() > MAX_EVENT_IDS {
            self.bulk_change = Some(now + LOCAL_CHANGE_TTL_MS);
        }
    }

    /// Whether an event about `ids` (`None`: unknown) is the echo of this tab's own changes.
    /// An event without ids is the echo of the pending bulk change, which it consumes.
    fn is_own_change(&mut self, ids: Option<&[NotificationId]>, now: f64) -> bool {
        self.prune(now);
        match ids {
            Some(ids) => {
                !ids.is_empty() && ids.iter().all(|id| self.notifications.contains_key(id))
            }
            None => self.bulk_change.take().is_some(),
        }
    }
}

/// A displayed list fetched from the API, with the section and filters it was fetched for.
struct FetchedList {
    section: NotificationSection,
    is_filtered: bool,
    total: usize,
}

impl FetchedList {
    /// The badge count the list gives (its `total`): its section's, when no source filter
    /// narrows it.
    fn section_count(&self) -> Option<(NotificationSection, usize)> {
        (!self.is_filtered).then_some((self.section, self.total))
    }
}

pub async fn notification_service(
    mut rx: UnboundedReceiver<NotificationCommand>,
    api_base_url: Url,
    mut notifications_page: Signal<Page<NotificationWithTask>>,
    notification_filters: Signal<NotificationFilters>,
    ui_model: Signal<UniversalInboxUIModel>,
    task_service: Coroutine<TaskCommand>,
    toast_service: Coroutine<ToastCommand>,
) {
    let mut local_changes = LocalChanges::default();
    loop {
        let msg = rx.next().await;
        // Most commands change the inbox/snoozed badge counts: explicit refreshes (initial
        // load + periodic tick), syncs, and any action moving a notification between
        // sections. Pure navigation (LoadAndSelect, RefreshList) and read-state (MarkAsRead,
        // which keeps the notification in the inbox) do not.
        // The list and the counts already reflect this tab's own changes
        let is_own_change_event = matches!(
            msg,
            Some(NotificationCommand::RefreshFromEvent(ref ids))
                if local_changes.is_own_change(ids.as_deref(), Date::now())
        );
        let refresh_counts = !is_own_change_event
            && !matches!(
                msg,
                None | Some(NotificationCommand::LoadAndSelect(_))
                    | Some(NotificationCommand::MarkAsRead(_))
                    | Some(NotificationCommand::RefreshList)
            );
        // The displayed list, when refetched: its total is up to date
        let mut fetched_list = None;
        // The notifications the command changed on the server
        let mut changed_ids: Vec<NotificationId> = vec![];
        match msg {
            Some(NotificationCommand::RefreshFromEvent(_)) if is_own_change_event => {}
            Some(NotificationCommand::Refresh)
            | Some(NotificationCommand::RefreshList)
            | Some(NotificationCommand::RefreshFromEvent(_)) => {
                fetched_list = refresh_notifications(
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
                if let Ok(notifications) = result {
                    changed_ids = notifications.iter().map(|notif| notif.id).collect();
                    fetched_list = refresh_notifications(
                        &api_base_url,
                        notifications_page,
                        notification_filters,
                        ui_model,
                    )
                    .await;
                }
            }
            Some(NotificationCommand::DeleteFromNotification(ref notification)) => {
                if let Some(ref task) = notification.task
                    && notification.is_built_from_task()
                {
                    delete_task(notification.id, task.id, notifications_page, &task_service).await;
                    // The task service deletes it: assume it succeeds
                    changed_ids.push(notification.id);
                } else if delete_notification(
                    &api_base_url,
                    notification.id,
                    notifications_page,
                    ui_model,
                    &toast_service,
                )
                .await
                {
                    changed_ids.push(notification.id);
                }
            }
            Some(NotificationCommand::DeleteAll) => {
                (changed_ids, fetched_list) = delete_all_notifications(
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

                let result: Result<Option<Notification>> = call_api_and_notify(
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
                if result.is_ok() {
                    changed_ids.push(notification_id);
                }
            }
            Some(NotificationCommand::Snooze(notification_id)) => {
                let snoozed_time = compute_snoozed_until(Local::now(), 1, 6);

                notifications_page
                    .write()
                    .remove_element(|notif| notif.id != notification_id);

                let result: Result<Option<Notification>> = call_api_and_notify(
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
                if result.is_ok() {
                    changed_ids.push(notification_id);
                }
            }
            Some(NotificationCommand::Unsnooze(notification_id)) => {
                notifications_page
                    .write()
                    .remove_element(|notif| notif.id != notification_id);

                // Setting `snoozed_until` to the past brings the notification back into
                // the inbox (the inbox filter keeps `snoozed_until <= now`) and out of
                // the snoozed list (which keeps `snoozed_until > now`).
                let result: Result<Option<Notification>> = call_api_and_notify(
                    Method::PATCH,
                    &api_base_url,
                    &format!("notifications/{notification_id}"),
                    Some(NotificationPatch {
                        snoozed_until: Some(Utc::now()),
                        ..Default::default()
                    }),
                    Some(ui_model),
                    &toast_service,
                    "Unsnoozing notification...",
                    "Successfully unsnoozed notification",
                )
                .await;
                if result.is_ok() {
                    changed_ids.push(notification_id);
                }
            }
            Some(NotificationCommand::Undelete(notification_id)) => {
                notifications_page
                    .write()
                    .remove_element(|notif| notif.id != notification_id);

                let result: Result<Option<Notification>> = call_api_and_notify(
                    Method::PATCH,
                    &api_base_url,
                    &format!("notifications/{notification_id}"),
                    Some(NotificationPatch {
                        status: Some(NotificationStatus::Unread),
                        ..Default::default()
                    }),
                    Some(ui_model),
                    &toast_service,
                    "Restoring notification...",
                    "Successfully restored notification",
                )
                .await;
                if result.is_ok() {
                    changed_ids.push(notification_id);
                }
            }
            Some(NotificationCommand::LoadAndSelect(notification_id)) => {
                load_and_select_notification(
                    &api_base_url,
                    notification_id,
                    notifications_page,
                    notification_filters,
                    ui_model,
                )
                .await;
            }
            Some(NotificationCommand::MarkAsRead(notification_id)) => {
                let result: Result<Option<Notification>> = call_api(
                    Method::PATCH,
                    &api_base_url,
                    &format!("notifications/{notification_id}"),
                    Some(NotificationPatch {
                        status: Some(NotificationStatus::Read),
                        ..Default::default()
                    }),
                    Some(ui_model),
                )
                .await;
                if result.is_ok() {
                    changed_ids.push(notification_id);
                    let mut page = notifications_page.write();
                    if let Some(notif) = page.content.iter_mut().find(|n| n.id == notification_id) {
                        notif.status = NotificationStatus::Read;
                        notif.last_read_at = Some(Utc::now());
                    }
                }
            }
            Some(NotificationCommand::CompleteTaskFromNotification(ref notification)) => {
                if let Some(ref task) = notification.task
                    && notification.is_built_from_task()
                {
                    notifications_page
                        .write()
                        .remove_element(|notif| notif.id != notification.id);

                    task_service.send(TaskCommand::Complete(task.id));
                    // The task service completes it: assume it succeeds
                    changed_ids.push(notification.id);
                }
            }
            Some(NotificationCommand::PlanTask(ref notification, task_id, parameters)) => {
                notifications_page
                    .write()
                    .remove_element(|notif| notif.id != notification.id);

                task_service.send(TaskCommand::Plan(task_id, parameters));
                // The task service plans it: assume it succeeds
                changed_ids.push(notification.id);
            }
            Some(NotificationCommand::CreateTaskWithDetaultsFromNotification(ref notification)) => {
                notifications_page
                    .write()
                    .remove_element(|notif| notif.id != notification.id);

                let result: Result<Option<NotificationWithTask>> = call_api_and_notify(
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
                if result.is_ok() {
                    changed_ids.push(notification.id);
                }
            }
            Some(NotificationCommand::CreateTaskFromNotification(ref notification, parameters)) => {
                notifications_page
                    .write()
                    .remove_element(|notif| notif.id != notification.id);

                let result: Result<Option<NotificationWithTask>> = call_api_and_notify(
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
                if result.is_ok() {
                    changed_ids.push(notification.id);
                }
            }
            Some(NotificationCommand::LinkNotificationWithTask(notification_id, task_id)) => {
                notifications_page
                    .write()
                    .remove_element(|notif| notif.id != notification_id);

                let result: Result<Option<NotificationWithTask>> = call_api_and_notify(
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
                if result.is_ok() {
                    changed_ids.push(notification_id);
                }
            }
            Some(NotificationCommand::AcceptInvitation(notification_id)) => {
                let answered = patch_invitation(
                    notifications_page,
                    notification_id,
                    &api_base_url,
                    GoogleCalendarEventAttendeeResponseStatus::Accepted,
                    ui_model,
                    toast_service,
                )
                .await;
                changed_ids.extend(answered.then_some(notification_id));
            }
            Some(NotificationCommand::DeclineInvitation(notification_id)) => {
                let answered = patch_invitation(
                    notifications_page,
                    notification_id,
                    &api_base_url,
                    GoogleCalendarEventAttendeeResponseStatus::Declined,
                    ui_model,
                    toast_service,
                )
                .await;
                changed_ids.extend(answered.then_some(notification_id));
            }
            Some(NotificationCommand::TentativelyAcceptInvitation(notification_id)) => {
                let answered = patch_invitation(
                    notifications_page,
                    notification_id,
                    &api_base_url,
                    GoogleCalendarEventAttendeeResponseStatus::Tentative,
                    ui_model,
                    toast_service,
                )
                .await;
                changed_ids.extend(answered.then_some(notification_id));
            }
            None => {}
        }
        local_changes.record(&changed_ids, Date::now());

        if refresh_counts {
            if let Some(refilled_list) = maybe_refill_current_page(
                &api_base_url,
                notifications_page,
                notification_filters,
                ui_model,
            )
            .await
            {
                fetched_list = Some(refilled_list);
            }
            let section_count = fetched_list.as_ref().and_then(FetchedList::section_count);
            refresh_notification_counts(&api_base_url, ui_model, section_count).await;
        }
    }
}

/// When more than half of the current page's notifications have been removed (deleted,
/// snoozed, ...) and there are further pages to pull from, refetch the current page to
/// refill the list. Skips the last page (nothing to pull up) and keeps the currently
/// selected notification selected across the refetch. Returns the list it refetched.
async fn maybe_refill_current_page(
    api_base_url: &Url,
    notifications_page: Signal<Page<NotificationWithTask>>,
    notification_filters: Signal<NotificationFilters>,
    mut ui_model: Signal<UniversalInboxUIModel>,
) -> Option<FetchedList> {
    let (per_page, pages_count, len) = {
        let page = notifications_page.read();
        (page.per_page, page.pages_count, page.content.len())
    };
    if per_page == 0 || pages_count <= 1 {
        return None;
    }

    let offset = match notification_filters.read().current_page_token {
        PageToken::Offset(offset) => offset,
        // Offset is the only token kind pagination produces; treat anything else as page 1.
        _ => 0,
    };
    let current_page_index = offset / per_page;
    let is_last_page = current_page_index + 1 >= pages_count;
    // More than half of a (full, non-last) page gone => fewer than half remain.
    let more_than_half_removed = len * 2 < per_page;

    if is_last_page || !more_than_half_removed {
        return None;
    }

    // Preserve the selected notification across the refetch by remembering its id.
    let selected_id = ui_model
        .read()
        .selected_notification_index
        .and_then(|index| notifications_page.read().content.get(index).map(|n| n.id));

    let fetched_list = refresh_notifications(
        api_base_url,
        notifications_page,
        notification_filters,
        ui_model,
    )
    .await;

    if let Some(id) = selected_id
        && let Some(index) = notifications_page
            .read()
            .content
            .iter()
            .position(|n| n.id == id)
    {
        ui_model.write().selected_notification_index = Some(index);
    }
    fetched_list
}

/// Build the status / snooze query parameters for a section's notification list.
/// - Inbox: `Unread`/`Read`, future-snoozed excluded (the default backend behaviour).
/// - Snoozed: `Unread`/`Read` restricted to `snoozed_until > now`.
/// - Deleted: `Deleted`, including snoozed ones so none are hidden.
fn section_list_parameters(section: NotificationSection) -> Vec<(&'static str, String)> {
    match section {
        NotificationSection::Inbox => vec![("status", "Unread,Read".to_string())],
        NotificationSection::Snoozed => vec![
            ("status", "Unread,Read".to_string()),
            ("only_snoozed_notifications", "true".to_string()),
        ],
        NotificationSection::Deleted => vec![
            ("status", "Deleted".to_string()),
            ("include_snoozed_notifications", "true".to_string()),
        ],
    }
}

/// Fetch the displayed list, returning it when it succeeded.
async fn refresh_notifications(
    api_base_url: &Url,
    mut notifications_page: Signal<Page<NotificationWithTask>>,
    notification_filters: Signal<NotificationFilters>,
    ui_model: Signal<UniversalInboxUIModel>,
) -> Option<FetchedList> {
    let section = CURRENT_NOTIFICATION_SECTION();
    let is_filtered = notification_filters().is_filtered();
    let source_filters: Vec<String> = notification_filters()
        .notification_source_kind_filters
        .iter()
        .filter(|f| f.selected)
        .map(|f| f.kind.to_string())
        .collect();

    let mut parameters = section_list_parameters(section);
    parameters.push(("with_tasks", "true".to_string()));
    parameters.push(("order_by", notification_filters().sort_by.to_string()));
    parameters.push(("sources", source_filters.join(",")));
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

    let new_notifications_page = result.ok()?;
    let total = new_notifications_page.total;
    *notifications_page.write() = new_notifications_page;
    Some(FetchedList {
        section,
        is_filtered,
        total,
    })
}

/// Refresh the inbox and snoozed sidebar badge counts independently of the currently
/// displayed list, so both badges stay correct on any section. `section_count`: a section
/// whose count is already known, from the list just fetched, and is not fetched again.
async fn refresh_notification_counts(
    api_base_url: &Url,
    ui_model: Signal<UniversalInboxUIModel>,
    section_count: Option<(NotificationSection, usize)>,
) {
    for (section, mut count_signal) in [
        (NotificationSection::Inbox, INBOX_COUNT.signal()),
        (NotificationSection::Snoozed, SNOOZED_COUNT.signal()),
    ] {
        if let Some((counted_section, count)) = section_count
            && counted_section == section
        {
            *count_signal.write() = count;
            continue;
        }
        let mut parameters = section_list_parameters(section);
        parameters.push(("trigger_sync", "false".to_string()));
        let filters = serde_urlencoded::to_string(&parameters).unwrap_or_default();
        let result: Result<Page<NotificationWithTask>> = call_api(
            Method::GET,
            api_base_url,
            &format!("notifications?{filters}"),
            None::<i32>,
            Some(ui_model),
        )
        .await;
        if let Ok(page) = result {
            *count_signal.write() = page.total;
        }
    }
}

/// Resolve a deep-linked notification: select it in the current list if present,
/// otherwise fetch it, switch to its section, and prepend it when it is off the first
/// page so it can be selected and previewed.
async fn load_and_select_notification(
    api_base_url: &Url,
    notification_id: NotificationId,
    mut notifications_page: Signal<Page<NotificationWithTask>>,
    notification_filters: Signal<NotificationFilters>,
    mut ui_model: Signal<UniversalInboxUIModel>,
) {
    if let Some(index) = notifications_page
        .read()
        .content
        .iter()
        .position(|notif| notif.id == notification_id)
    {
        ui_model.write().selected_notification_index = Some(index);
        return;
    }

    let result: Result<NotificationWithTask> = call_api(
        Method::GET,
        api_base_url,
        &format!("notifications/{notification_id}"),
        None::<i32>,
        Some(ui_model),
    )
    .await;
    let Ok(notification) = result else {
        return;
    };

    let section = section_of(&notification);
    if CURRENT_NOTIFICATION_SECTION() != section {
        *CURRENT_NOTIFICATION_SECTION.write() = section;
        refresh_notifications(
            api_base_url,
            notifications_page,
            notification_filters,
            ui_model,
        )
        .await;
    }

    if let Some(index) = notifications_page
        .read()
        .content
        .iter()
        .position(|notif| notif.id == notification_id)
    {
        ui_model.write().selected_notification_index = Some(index);
    } else {
        notifications_page.write().content.insert(0, notification);
        ui_model.write().selected_notification_index = Some(0);
    }
}

/// Delete a notification, returning whether it succeeded.
async fn delete_notification(
    api_base_url: &Url,
    notification_id: NotificationId,
    mut notifications_page: Signal<Page<NotificationWithTask>>,
    ui_model: Signal<UniversalInboxUIModel>,
    toast_service: &Coroutine<ToastCommand>,
) -> bool {
    notifications_page
        .write()
        .remove_element(|notif| notif.id != notification_id);

    let result: Result<Option<Notification>, anyhow::Error> = call_api_and_notify(
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
    result.is_ok()
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

/// Delete the inbox notifications, returning the deleted ones and the list refetched after.
async fn delete_all_notifications(
    api_base_url: &Url,
    mut notifications_page: Signal<Page<NotificationWithTask>>,
    notification_filters: Signal<NotificationFilters>,
    ui_model: Signal<UniversalInboxUIModel>,
    toast_service: &Coroutine<ToastCommand>,
) -> (Vec<NotificationId>, Option<FetchedList>) {
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

    let result: Result<Vec<Notification>> = call_api_and_notify(
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

    let Ok(deleted_notifications) = result else {
        return (vec![], None);
    };
    let deleted_ids = deleted_notifications
        .into_iter()
        .map(|notification| notification.id)
        .collect();
    // Refresh to make sure UI is in sync
    let fetched_list = refresh_notifications(
        api_base_url,
        notifications_page,
        notification_filters,
        ui_model,
    )
    .await;
    (deleted_ids, fetched_list)
}

/// Answer an invitation, returning whether it succeeded.
async fn patch_invitation(
    mut notifications_page: Signal<Page<NotificationWithTask>>,
    notification_id: NotificationId,
    api_base_url: &Url,
    response_status: GoogleCalendarEventAttendeeResponseStatus,
    ui_model: Signal<UniversalInboxUIModel>,
    toast_service: Coroutine<ToastCommand>,
) -> bool {
    // An attendee reply (METHOD:REPLY) is not an invitation to answer
    let has_invitation = notifications_page.read().content.iter().any(|notif| {
        notif.id == notification_id
            && matches!(
                &notif.source_item.data,
                ThirdPartyItemData::GoogleCalendarEvent(event) if event.method != EventMethod::Reply
            )
    });
    if !has_invitation {
        return false;
    }

    notifications_page
        .write()
        .remove_element(|notif| notif.id != notification_id);

    let (loading_message, success_message) = match response_status {
        GoogleCalendarEventAttendeeResponseStatus::Accepted => (
            "Accepting invitation...",
            "Invitation successfully accepted",
        ),
        GoogleCalendarEventAttendeeResponseStatus::Declined => (
            "Declining invitation...",
            "Invitation successfully declined",
        ),
        GoogleCalendarEventAttendeeResponseStatus::Tentative => (
            "Tentatively accepting invitation...",
            "Invitation successfully tentatively accepted",
        ),
        GoogleCalendarEventAttendeeResponseStatus::NeedsAction => {
            ("Updating invitation...", "Invitation successfully updated")
        }
    };

    let result: Result<Option<Notification>> = call_api_and_notify(
        Method::PATCH,
        api_base_url,
        &format!("notifications/{notification_id}/invitation"),
        Some(InvitationPatch { response_status }),
        Some(ui_model),
        &toast_service,
        loading_message,
        success_message,
    )
    .await;
    result.is_ok()
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

    pub fn reset(&mut self) {
        for f in &mut self.notification_source_kind_filters {
            f.selected = true;
        }
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
        notification_source_kind_filters.sort_by_key(|a| a.kind.to_string());

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

    #[wasm_bindgen_test]
    fn test_section_list_parameters_inbox() {
        assert_eq!(
            section_list_parameters(NotificationSection::Inbox),
            vec![("status", "Unread,Read".to_string())]
        );
    }

    #[wasm_bindgen_test]
    fn test_section_list_parameters_snoozed() {
        assert_eq!(
            section_list_parameters(NotificationSection::Snoozed),
            vec![
                ("status", "Unread,Read".to_string()),
                ("only_snoozed_notifications", "true".to_string()),
            ]
        );
    }

    fn notification_id(n: u128) -> NotificationId {
        NotificationId(uuid::Uuid::from_u128(n))
    }

    #[wasm_bindgen_test]
    fn test_local_changes_skip_echo_of_own_changes() {
        let mut local_changes = LocalChanges::default();
        local_changes.record(&[notification_id(1)], 0.0);
        local_changes.record(&[notification_id(2)], 0.0);

        assert!(local_changes.is_own_change(Some(&[notification_id(1)]), 1_000.0));
        assert!(
            local_changes.is_own_change(Some(&[notification_id(1), notification_id(2)]), 1_000.0)
        );
        // A change made elsewhere
        assert!(
            !local_changes.is_own_change(Some(&[notification_id(1), notification_id(3)]), 1_000.0)
        );
        assert!(!local_changes.is_own_change(None, 1_000.0));
        assert!(!local_changes.is_own_change(Some(&[]), 1_000.0));
    }

    #[wasm_bindgen_test]
    fn test_local_changes_expire() {
        let mut local_changes = LocalChanges::default();
        local_changes.record(&[notification_id(1)], 0.0);

        assert!(!local_changes.is_own_change(Some(&[notification_id(1)]), LOCAL_CHANGE_TTL_MS));
    }

    #[wasm_bindgen_test]
    fn test_local_changes_are_pruned_when_recording() {
        let mut local_changes = LocalChanges::default();
        local_changes.record(&[notification_id(1)], 0.0);
        local_changes.record(&[], LOCAL_CHANGE_TTL_MS);

        assert!(local_changes.notifications.is_empty());
    }

    #[wasm_bindgen_test]
    fn test_local_changes_of_too_many_notifications_skip_the_next_event_without_ids_once() {
        let ids: Vec<NotificationId> = (0..=MAX_EVENT_IDS as u128).map(notification_id).collect();
        let mut local_changes = LocalChanges::default();
        local_changes.record(&ids, 0.0);

        assert!(local_changes.is_own_change(None, 1_000.0));
        assert!(!local_changes.is_own_change(None, 1_000.0));
        // An event with ids is judged by its ids only
        assert!(!local_changes.is_own_change(Some(&[notification_id(1_000)]), 1_000.0));

        local_changes.record(&ids, 0.0);
        assert!(!local_changes.is_own_change(None, LOCAL_CHANGE_TTL_MS));
    }

    #[wasm_bindgen_test]
    fn test_local_changes_of_few_notifications_do_not_skip_events_without_ids() {
        let ids: Vec<NotificationId> = (0..MAX_EVENT_IDS as u128).map(notification_id).collect();
        let mut local_changes = LocalChanges::default();
        local_changes.record(&ids, 0.0);
        // A sync or a delete all changing nothing
        local_changes.record(&[], 0.0);

        assert!(local_changes.is_own_change(Some(&ids), 1_000.0));
        assert!(!local_changes.is_own_change(None, 1_000.0));
    }

    #[wasm_bindgen_test]
    fn test_fetched_list_section_count() {
        let list = FetchedList {
            section: NotificationSection::Snoozed,
            is_filtered: false,
            total: 3,
        };
        assert_eq!(
            list.section_count(),
            Some((NotificationSection::Snoozed, 3))
        );
        // A filtered list counts only some sources
        assert_eq!(
            FetchedList {
                is_filtered: true,
                ..list
            }
            .section_count(),
            None
        );
    }

    #[wasm_bindgen_test]
    fn test_section_list_parameters_deleted() {
        assert_eq!(
            section_list_parameters(NotificationSection::Deleted),
            vec![
                ("status", "Deleted".to_string()),
                ("include_snoozed_notifications", "true".to_string()),
            ]
        );
    }
}
