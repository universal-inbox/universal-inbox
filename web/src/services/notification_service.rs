use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    rc::Rc,
};

use anyhow::Result;
use chrono::{DateTime, Local, TimeDelta, TimeZone, Timelike, Utc};
use dioxus::prelude::*;
use futures_util::{
    FutureExt, StreamExt,
    future::{Fuse, FusedFuture, LocalBoxFuture},
    select,
    stream::FuturesUnordered,
};
use js_sys::Date;
use reqwest::Method;
use serde::{Serialize, de::DeserializeOwned};
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
    pending: PendingChanges,
}

impl LocalChanges {
    fn prune(&mut self, now: f64) {
        self.notifications.retain(|_, expires_at| *expires_at > now);
        self.bulk_change = self.bulk_change.filter(|expires_at| *expires_at > now);
    }

    /// Record the notifications this tab just changed on the server.
    fn record(&mut self, ids: &[NotificationId], now: f64) {
        self.record_changes(ids, ids.len() > MAX_EVENT_IDS, now);
    }

    /// Settle a change this tab started, recording the notifications it changed. The echo of
    /// a bulk change seen while it ran is not awaited again.
    fn settle(&mut self, change: u64, succeeded: bool, ids: &[NotificationId], now: f64) {
        let bulk_echo_seen = self.pending.settle(change, succeeded);
        self.record_changes(ids, ids.len() > MAX_EVENT_IDS && !bulk_echo_seen, now);
    }

    fn record_changes(&mut self, ids: &[NotificationId], is_bulk_change: bool, now: f64) {
        self.prune(now);
        for id in ids {
            self.notifications.insert(*id, now + LOCAL_CHANGE_TTL_MS);
        }
        if is_bulk_change {
            self.bulk_change = Some(now + LOCAL_CHANGE_TTL_MS);
        }
    }

    /// Whether an event about `ids` (`None`: unknown) is the echo of this tab's own changes.
    /// An event without ids is the echo of the pending bulk change, which it consumes.
    fn is_own_change(&mut self, ids: Option<&[NotificationId]>, now: f64) -> bool {
        self.prune(now);
        match ids {
            Some(ids) => {
                !ids.is_empty()
                    && ids.iter().all(|id| {
                        self.notifications.contains_key(id) || self.pending.is_running(*id)
                    })
            }
            None => self.pending.take_running_bulk_echo() || self.bulk_change.take().is_some(),
        }
    }
}

/// The local effect of a change this tab started, kept on the lists fetched while it runs.
#[derive(Debug, Clone, Copy, PartialEq)]
enum PendingEffect {
    Removed(NotificationId, NotificationSection),
    RemovedAll(NotificationSection),
    Read(NotificationId),
}

#[derive(Debug)]
struct PendingChange {
    effect: PendingEffect,
    /// The last list fetch started before the change's API call returned, `None` while it runs
    settled_after: Option<u64>,
    bulk_echo_seen: bool,
}

impl PendingChange {
    fn applies_to(&self, list_fetch: u64) -> bool {
        self.settled_after
            .is_none_or(|settled_after| settled_after >= list_fetch)
    }
}

/// The changes this tab started whose effect a fetched list may not reflect yet.
#[derive(Debug, Default)]
struct PendingChanges {
    changes: HashMap<u64, PendingChange>,
    last_change: u64,
    list_fetches: u64,
}

impl PendingChanges {
    fn start(&mut self, effect: PendingEffect) -> u64 {
        self.last_change += 1;
        self.changes.insert(
            self.last_change,
            PendingChange {
                effect,
                settled_after: None,
                bulk_echo_seen: false,
            },
        );
        self.last_change
    }

    /// Settle a change, returning whether the echo of its bulk change was seen while it ran.
    fn settle(&mut self, change: u64, succeeded: bool) -> bool {
        let bulk_echo_seen = self
            .changes
            .get(&change)
            .is_some_and(|pending_change| pending_change.bulk_echo_seen);
        if !succeeded {
            self.changes.remove(&change);
        } else if let Some(pending_change) = self.changes.get_mut(&change) {
            pending_change.settled_after = Some(self.list_fetches);
        }
        bulk_echo_seen
    }

    fn start_list_fetch(&mut self) -> u64 {
        self.list_fetches += 1;
        let list_fetch = self.list_fetches;
        self.changes
            .retain(|_, pending_change| pending_change.applies_to(list_fetch));
        list_fetch
    }

    fn effects_on(&self, list_fetch: u64, section: NotificationSection) -> Vec<PendingEffect> {
        self.changes
            .values()
            .filter(|pending_change| pending_change.applies_to(list_fetch))
            .map(|pending_change| pending_change.effect)
            .filter(|effect| match effect {
                PendingEffect::Removed(_, removed_from)
                | PendingEffect::RemovedAll(removed_from) => *removed_from == section,
                PendingEffect::Read(_) => true,
            })
            .collect()
    }

    fn apply(
        &self,
        list_fetch: u64,
        section: NotificationSection,
        page: &mut Page<NotificationWithTask>,
    ) {
        for effect in self.effects_on(list_fetch, section) {
            match effect {
                PendingEffect::Removed(id, _) => {
                    page.remove_element(|notification| notification.id != id)
                }
                PendingEffect::RemovedAll(_) => page.content.clear(),
                PendingEffect::Read(id) => {
                    if let Some(notification) = page.content.iter_mut().find(|notification| {
                        notification.id == id && notification.status == NotificationStatus::Unread
                    }) {
                        notification.status = NotificationStatus::Read;
                        notification.last_read_at = Some(Utc::now());
                    }
                }
            }
        }
    }

    fn running_effects(&self) -> impl Iterator<Item = PendingEffect> + '_ {
        self.changes
            .values()
            .filter(|pending_change| pending_change.settled_after.is_none())
            .map(|pending_change| pending_change.effect)
    }

    fn is_running(&self, id: NotificationId) -> bool {
        self.running_effects().any(|effect| {
            matches!(effect, PendingEffect::Removed(changed_id, _) | PendingEffect::Read(changed_id) if changed_id == id)
        })
    }

    /// Whether a running bulk change awaits its echo, which this call consumes.
    fn take_running_bulk_echo(&mut self) -> bool {
        self.changes
            .values_mut()
            .find(|pending_change| {
                pending_change.settled_after.is_none()
                    && !pending_change.bulk_echo_seen
                    && matches!(pending_change.effect, PendingEffect::RemovedAll(_))
            })
            .map(|pending_change| pending_change.bulk_echo_seen = true)
            .is_some()
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

#[derive(Clone)]
struct NotificationServiceContext {
    api_base_url: Url,
    notifications_page: Signal<Page<NotificationWithTask>>,
    notification_filters: Signal<NotificationFilters>,
    ui_model: Signal<UniversalInboxUIModel>,
    task_service: Coroutine<TaskCommand>,
    toast_service: Coroutine<ToastCommand>,
    local_changes: Rc<RefCell<LocalChanges>>,
}

/// Work on the displayed list and the badge counts, run one at a time in order.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ListJob {
    Refresh { refresh_counts: bool },
    LoadAndSelect(NotificationId),
    Sync(Option<NotificationSyncSourceKind>),
    RefreshCounts,
}

impl ListJob {
    fn refreshes_counts(&self) -> bool {
        matches!(
            self,
            ListJob::Refresh {
                refresh_counts: true
            } | ListJob::Sync(_)
                | ListJob::RefreshCounts
        )
    }
}

fn enqueue_list_job(list_jobs: &mut VecDeque<ListJob>, list_job: ListJob) {
    if list_job == ListJob::RefreshCounts && list_jobs.iter().any(ListJob::refreshes_counts) {
        return;
    }
    list_jobs.push_back(list_job);
}

enum StartedCommand {
    /// An API call, returning the job it leaves on the displayed list
    Action(LocalBoxFuture<'static, Option<ListJob>>),
    ListJob(ListJob),
    Nothing,
}

pub async fn notification_service(
    mut rx: UnboundedReceiver<NotificationCommand>,
    api_base_url: Url,
    notifications_page: Signal<Page<NotificationWithTask>>,
    notification_filters: Signal<NotificationFilters>,
    ui_model: Signal<UniversalInboxUIModel>,
    task_service: Coroutine<TaskCommand>,
    toast_service: Coroutine<ToastCommand>,
) {
    let context = NotificationServiceContext {
        api_base_url,
        notifications_page,
        notification_filters,
        ui_model,
        task_service,
        toast_service,
        local_changes: Rc::default(),
    };
    let mut actions: FuturesUnordered<LocalBoxFuture<'static, Option<ListJob>>> =
        FuturesUnordered::new();
    let mut list_jobs: VecDeque<ListJob> = VecDeque::new();
    let mut running_list_job: Fuse<LocalBoxFuture<'static, ()>> = Fuse::terminated();

    loop {
        select! {
            command = rx.next() => match command {
                Some(command) => match start_command(&context, command) {
                    StartedCommand::Action(action) => actions.push(action),
                    StartedCommand::ListJob(list_job) => enqueue_list_job(&mut list_jobs, list_job),
                    StartedCommand::Nothing => {}
                },
                None => break,
            },
            follow_up = actions.select_next_some() => {
                if let Some(list_job) = follow_up {
                    enqueue_list_job(&mut list_jobs, list_job);
                }
            },
            _ = &mut running_list_job => {},
        }
        if running_list_job.is_terminated()
            && let Some(list_job) = list_jobs.pop_front()
        {
            running_list_job = run_list_job(context.clone(), list_job).boxed_local().fuse();
        }
    }
}

/// Apply a command's local change at once and return the work it leaves: an API call run
/// alongside the other commands' ones, or a job on the displayed list.
fn start_command(
    context: &NotificationServiceContext,
    command: NotificationCommand,
) -> StartedCommand {
    let mut notifications_page = context.notifications_page;
    match command {
        NotificationCommand::Refresh => StartedCommand::ListJob(ListJob::Refresh {
            refresh_counts: true,
        }),
        NotificationCommand::RefreshList => StartedCommand::ListJob(ListJob::Refresh {
            refresh_counts: false,
        }),
        NotificationCommand::RefreshFromEvent(ids) => {
            if context
                .local_changes
                .borrow_mut()
                .is_own_change(ids.as_deref(), Date::now())
            {
                StartedCommand::Nothing
            } else {
                StartedCommand::ListJob(ListJob::Refresh {
                    refresh_counts: true,
                })
            }
        }
        NotificationCommand::Sync(source) => StartedCommand::ListJob(ListJob::Sync(source)),
        NotificationCommand::LoadAndSelect(notification_id) => {
            StartedCommand::ListJob(ListJob::LoadAndSelect(notification_id))
        }
        NotificationCommand::DeleteFromNotification(notification) => {
            if let Some(ref task) = notification.task
                && notification.is_built_from_task()
            {
                delete_task(
                    notification.id,
                    task.id,
                    notifications_page,
                    &context.task_service,
                );
                completed(context, notification.id)
            } else {
                remove_and_call::<_, Option<Notification>>(
                    context,
                    notification.id,
                    Method::PATCH,
                    format!("notifications/{}", notification.id),
                    Some(NotificationPatch {
                        status: Some(NotificationStatus::Deleted),
                        ..Default::default()
                    }),
                    "Deleting notification...",
                    "Successfully deleted notification",
                )
            }
        }
        NotificationCommand::DeleteAll => delete_all_notifications(context),
        NotificationCommand::Unsubscribe(notification_id) => {
            remove_and_call::<_, Option<Notification>>(
                context,
                notification_id,
                Method::PATCH,
                format!("notifications/{notification_id}"),
                Some(NotificationPatch {
                    status: Some(NotificationStatus::Unsubscribed),
                    ..Default::default()
                }),
                "Unsubscribing from notification...",
                "Successfully unsubscribed from notification",
            )
        }
        NotificationCommand::Snooze(notification_id) => {
            let snoozed_time = compute_snoozed_until(Local::now(), 1, 6);
            remove_and_call::<_, Option<Notification>>(
                context,
                notification_id,
                Method::PATCH,
                format!("notifications/{notification_id}"),
                Some(NotificationPatch {
                    snoozed_until: Some(snoozed_time),
                    ..Default::default()
                }),
                "Snoozing notification...",
                "Successfully snoozed notification",
            )
        }
        // Setting `snoozed_until` to the past brings the notification back into the inbox
        // (the inbox filter keeps `snoozed_until <= now`) and out of the snoozed list
        // (which keeps `snoozed_until > now`).
        NotificationCommand::Unsnooze(notification_id) => {
            remove_and_call::<_, Option<Notification>>(
                context,
                notification_id,
                Method::PATCH,
                format!("notifications/{notification_id}"),
                Some(NotificationPatch {
                    snoozed_until: Some(Utc::now()),
                    ..Default::default()
                }),
                "Unsnoozing notification...",
                "Successfully unsnoozed notification",
            )
        }
        NotificationCommand::Undelete(notification_id) => {
            remove_and_call::<_, Option<Notification>>(
                context,
                notification_id,
                Method::PATCH,
                format!("notifications/{notification_id}"),
                Some(NotificationPatch {
                    status: Some(NotificationStatus::Unread),
                    ..Default::default()
                }),
                "Restoring notification...",
                "Successfully restored notification",
            )
        }
        NotificationCommand::MarkAsRead(notification_id) => {
            mark_notification_as_read(context, notification_id)
        }
        NotificationCommand::CompleteTaskFromNotification(notification) => {
            if let Some(ref task) = notification.task
                && notification.is_built_from_task()
            {
                notifications_page
                    .write()
                    .remove_element(|notif| notif.id != notification.id);

                context.task_service.send(TaskCommand::Complete(task.id));
                completed(context, notification.id)
            } else {
                StartedCommand::ListJob(ListJob::RefreshCounts)
            }
        }
        NotificationCommand::PlanTask(notification, task_id, parameters) => {
            notifications_page
                .write()
                .remove_element(|notif| notif.id != notification.id);

            context
                .task_service
                .send(TaskCommand::Plan(task_id, parameters));
            completed(context, notification.id)
        }
        NotificationCommand::CreateTaskWithDetaultsFromNotification(notification) => {
            remove_and_call::<_, Option<NotificationWithTask>>(
                context,
                notification.id,
                Method::POST,
                format!("notifications/{}/task", notification.id),
                None::<TaskCreation>,
                "Creating task with default settings from notification...",
                "Task successfully created",
            )
        }
        NotificationCommand::CreateTaskFromNotification(notification, parameters) => {
            remove_and_call::<_, Option<NotificationWithTask>>(
                context,
                notification.id,
                Method::POST,
                format!("notifications/{}/task", notification.id),
                Some(parameters),
                "Creating task from notification...",
                "Task successfully created",
            )
        }
        NotificationCommand::LinkNotificationWithTask(notification_id, task_id) => {
            remove_and_call::<_, Option<NotificationWithTask>>(
                context,
                notification_id,
                Method::PATCH,
                format!("notifications/{notification_id}"),
                Some(NotificationPatch {
                    status: Some(NotificationStatus::Deleted),
                    task_id: Some(task_id),
                    ..Default::default()
                }),
                "Linking notification...",
                "Notification successfully linked",
            )
        }
        NotificationCommand::AcceptInvitation(notification_id) => answer_invitation(
            context,
            notification_id,
            GoogleCalendarEventAttendeeResponseStatus::Accepted,
        ),
        NotificationCommand::DeclineInvitation(notification_id) => answer_invitation(
            context,
            notification_id,
            GoogleCalendarEventAttendeeResponseStatus::Declined,
        ),
        NotificationCommand::TentativelyAcceptInvitation(notification_id) => answer_invitation(
            context,
            notification_id,
            GoogleCalendarEventAttendeeResponseStatus::Tentative,
        ),
    }
}

/// A change another service makes: assume it succeeds.
fn completed(
    context: &NotificationServiceContext,
    notification_id: NotificationId,
) -> StartedCommand {
    context
        .local_changes
        .borrow_mut()
        .record(&[notification_id], Date::now());
    StartedCommand::ListJob(ListJob::RefreshCounts)
}

/// Settle a change to a notification once its API call returned.
fn settle(
    context: &NotificationServiceContext,
    change: u64,
    notification_id: NotificationId,
    succeeded: bool,
) {
    context.local_changes.borrow_mut().settle(
        change,
        succeeded,
        succeeded.then_some(notification_id).as_slice(),
        Date::now(),
    );
}

/// Remove a notification from the displayed list, then call the API to change it.
fn remove_and_call<B, R>(
    context: &NotificationServiceContext,
    notification_id: NotificationId,
    method: Method,
    path: String,
    body: Option<B>,
    loading_message: &'static str,
    success_message: &'static str,
) -> StartedCommand
where
    B: Serialize + 'static,
    R: DeserializeOwned + 'static,
{
    let section = *CURRENT_NOTIFICATION_SECTION.peek();
    let mut notifications_page = context.notifications_page;
    notifications_page
        .write()
        .remove_element(|notif| notif.id != notification_id);
    let change = context
        .local_changes
        .borrow_mut()
        .pending
        .start(PendingEffect::Removed(notification_id, section));

    let context = context.clone();
    StartedCommand::Action(
        async move {
            let result: Result<R> = call_api_and_notify(
                method,
                &context.api_base_url,
                &path,
                body,
                Some(context.ui_model),
                &context.toast_service,
                loading_message,
                success_message,
            )
            .await;
            settle(&context, change, notification_id, result.is_ok());
            Some(ListJob::RefreshCounts)
        }
        .boxed_local(),
    )
}

fn mark_notification_as_read(
    context: &NotificationServiceContext,
    notification_id: NotificationId,
) -> StartedCommand {
    let change = context
        .local_changes
        .borrow_mut()
        .pending
        .start(PendingEffect::Read(notification_id));

    let context = context.clone();
    StartedCommand::Action(
        async move {
            let result: Result<Option<Notification>> = call_api(
                Method::PATCH,
                &context.api_base_url,
                &format!("notifications/{notification_id}"),
                Some(NotificationPatch {
                    status: Some(NotificationStatus::Read),
                    ..Default::default()
                }),
                Some(context.ui_model),
            )
            .await;
            if result.is_ok() {
                let mut notifications_page = context.notifications_page;
                let mut page = notifications_page.write();
                if let Some(notif) = page.content.iter_mut().find(|n| n.id == notification_id) {
                    notif.status = NotificationStatus::Read;
                    notif.last_read_at = Some(Utc::now());
                }
            }
            settle(&context, change, notification_id, result.is_ok());
            None
        }
        .boxed_local(),
    )
}

/// Run a job on the displayed list, then refresh the badge counts when the job changes them.
async fn run_list_job(context: NotificationServiceContext, list_job: ListJob) {
    let fetched_list = match list_job {
        ListJob::Refresh { .. } => refresh_notifications(&context).await,
        ListJob::LoadAndSelect(notification_id) => {
            load_and_select_notification(&context, notification_id).await;
            None
        }
        ListJob::Sync(source) => sync_notifications(&context, source).await,
        ListJob::RefreshCounts => None,
    };

    if list_job.refreshes_counts() {
        let fetched_list = maybe_refill_current_page(&context).await.or(fetched_list);
        let section_count = fetched_list.as_ref().and_then(FetchedList::section_count);
        refresh_notification_counts(&context.api_base_url, context.ui_model, section_count).await;
    }
}

/// Sync notifications, returning the list refetched after.
async fn sync_notifications(
    context: &NotificationServiceContext,
    source: Option<NotificationSyncSourceKind>,
) -> Option<FetchedList> {
    let result: Result<Vec<Notification>> = call_api_and_notify(
        Method::POST,
        &context.api_base_url,
        "notifications/sync",
        Some(SyncNotificationsParameters {
            source,
            asynchronous: Some(false),
        }),
        Some(context.ui_model),
        &context.toast_service,
        "Syncing notifications...",
        "Successfully synced notifications",
    )
    .await;
    let notifications = result.ok()?;
    let changed_ids: Vec<NotificationId> = notifications.iter().map(|notif| notif.id).collect();
    context
        .local_changes
        .borrow_mut()
        .record(&changed_ids, Date::now());
    refresh_notifications(context).await
}

/// When more than half of the current page's notifications have been removed (deleted,
/// snoozed, ...) and there are further pages to pull from, refetch the current page to
/// refill the list. Skips the last page (nothing to pull up) and keeps the currently
/// selected notification selected across the refetch. Returns the list it refetched.
async fn maybe_refill_current_page(context: &NotificationServiceContext) -> Option<FetchedList> {
    let (per_page, pages_count, len) = {
        let page = context.notifications_page.read();
        (page.per_page, page.pages_count, page.content.len())
    };
    if per_page == 0 || pages_count <= 1 {
        return None;
    }

    let offset = match context.notification_filters.read().current_page_token {
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

    refresh_notifications(context).await
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

/// The index of the selected notification in a refreshed list, `None` when it left the
/// list (the index is then left as is, and clamped by the notifications page).
fn selected_index_after_refresh(
    selected_id: NotificationId,
    refreshed_ids: impl IntoIterator<Item = NotificationId>,
) -> Option<usize> {
    refreshed_ids.into_iter().position(|id| id == selected_id)
}

/// Fetch the displayed list, returning it when it succeeded. The selected notification
/// stays selected when it is still in the list, and the changes this tab started that the
/// fetched list may not reflect yet are applied to it.
async fn refresh_notifications(context: &NotificationServiceContext) -> Option<FetchedList> {
    let mut notifications_page = context.notifications_page;
    let notification_filters = context.notification_filters;
    let mut ui_model = context.ui_model;
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
    let list_fetch = context
        .local_changes
        .borrow_mut()
        .pending
        .start_list_fetch();
    let result: Result<Page<NotificationWithTask>> = call_api(
        Method::GET,
        &context.api_base_url,
        &format!("notifications?{filters}"),
        // random type as we don't care about the body's type
        None::<i32>,
        Some(ui_model),
    )
    .await;

    let mut new_notifications_page = result.ok()?;
    context
        .local_changes
        .borrow()
        .pending
        .apply(list_fetch, section, &mut new_notifications_page);
    let total = new_notifications_page.total;
    let selected_id = ui_model
        .peek()
        .selected_notification_index
        .and_then(|index| {
            notifications_page
                .peek()
                .content
                .get(index)
                .map(|notification| notification.id)
        });
    let new_selected_index = selected_id.and_then(|id| {
        selected_index_after_refresh(
            id,
            new_notifications_page
                .content
                .iter()
                .map(|notification| notification.id),
        )
    });
    *notifications_page.write() = new_notifications_page;
    // The selection follows the notification, not its former position: a refresh can
    // move it (eg. a sync changed its `updated_at`). Written in the same tick as the
    // list, so the state → URL effect never sees another notification at the old index.
    if let Some(index) = new_selected_index {
        ui_model.write().selected_notification_index = Some(index);
    }
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
    context: &NotificationServiceContext,
    notification_id: NotificationId,
) {
    let mut notifications_page = context.notifications_page;
    let mut ui_model = context.ui_model;
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
        &context.api_base_url,
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
        refresh_notifications(context).await;
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

fn delete_task(
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

/// Delete the inbox notifications, then refetch the list.
fn delete_all_notifications(context: &NotificationServiceContext) -> StartedCommand {
    let mut notifications_page = context.notifications_page;
    // Get current filter parameters to match what's currently visible
    let source_filters = context.notification_filters.peek().selected();

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
    let change = context
        .local_changes
        .borrow_mut()
        .pending
        .start(PendingEffect::RemovedAll(
            *CURRENT_NOTIFICATION_SECTION.peek(),
        ));

    let context = context.clone();
    StartedCommand::Action(
        async move {
            let result: Result<Vec<Notification>> = call_api_and_notify(
                Method::PATCH,
                &context.api_base_url,
                "notifications",
                Some(patch_request),
                Some(context.ui_model),
                &context.toast_service,
                "Deleting all notifications...",
                "All notifications have been deleted",
            )
            .await;

            let deleted_ids: Vec<NotificationId> = result
                .as_ref()
                .map(|deleted_notifications| {
                    deleted_notifications
                        .iter()
                        .map(|notification| notification.id)
                        .collect()
                })
                .unwrap_or_default();
            context.local_changes.borrow_mut().settle(
                change,
                result.is_ok(),
                &deleted_ids,
                Date::now(),
            );
            if result.is_ok() {
                // Refresh to make sure UI is in sync
                Some(ListJob::Refresh {
                    refresh_counts: true,
                })
            } else {
                Some(ListJob::RefreshCounts)
            }
        }
        .boxed_local(),
    )
}

/// Answer an invitation.
fn answer_invitation(
    context: &NotificationServiceContext,
    notification_id: NotificationId,
    response_status: GoogleCalendarEventAttendeeResponseStatus,
) -> StartedCommand {
    // An attendee reply (METHOD:REPLY) is not an invitation to answer
    let has_invitation = context.notifications_page.read().content.iter().any(|notif| {
        notif.id == notification_id
            && matches!(
                &notif.source_item.data,
                ThirdPartyItemData::GoogleCalendarEvent(event) if event.method != EventMethod::Reply
            )
    });
    if !has_invitation {
        return StartedCommand::ListJob(ListJob::RefreshCounts);
    }

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

    remove_and_call::<_, Option<Notification>>(
        context,
        notification_id,
        Method::PATCH,
        format!("notifications/{notification_id}/invitation"),
        Some(InvitationPatch { response_status }),
        loading_message,
        success_message,
    )
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

    #[wasm_bindgen_test]
    fn test_selected_index_after_refresh_follows_moved_notification() {
        let ids: Vec<NotificationId> = (1..=5)
            .map(|n| NotificationId(uuid::Uuid::from_u128(n)))
            .collect();
        // The selected notification was first; a sync bumped its `updated_at`, so the
        // refreshed list (oldest first) now has it last.
        let selected_id = ids[0];
        let refreshed_ids = [ids[1], ids[2], ids[3], ids[4], ids[0]];

        assert_eq!(
            selected_index_after_refresh(selected_id, refreshed_ids),
            Some(4)
        );
    }

    #[wasm_bindgen_test]
    fn test_selected_index_after_refresh_when_notification_left_the_list() {
        let ids: Vec<NotificationId> = (1..=3)
            .map(|n| NotificationId(uuid::Uuid::from_u128(n)))
            .collect();

        assert_eq!(selected_index_after_refresh(ids[0], [ids[1], ids[2]]), None);
    }

    fn removed_from_inbox(n: u128) -> PendingEffect {
        PendingEffect::Removed(notification_id(n), NotificationSection::Inbox)
    }

    #[wasm_bindgen_test]
    fn test_pending_changes_running_change_applies_to_fetched_list() {
        let mut pending = PendingChanges::default();
        pending.start(removed_from_inbox(1));
        pending.start(PendingEffect::Read(notification_id(2)));
        let list_fetch = pending.start_list_fetch();

        let effects = pending.effects_on(list_fetch, NotificationSection::Inbox);
        assert_eq!(effects.len(), 2);
        assert!(effects.contains(&removed_from_inbox(1)));
        assert!(effects.contains(&PendingEffect::Read(notification_id(2))));
    }

    #[wasm_bindgen_test]
    fn test_pending_changes_removal_applies_only_to_its_section() {
        let mut pending = PendingChanges::default();
        pending.start(removed_from_inbox(1));
        pending.start(PendingEffect::RemovedAll(NotificationSection::Inbox));
        let list_fetch = pending.start_list_fetch();

        assert!(
            pending
                .effects_on(list_fetch, NotificationSection::Deleted)
                .is_empty()
        );
    }

    #[wasm_bindgen_test]
    fn test_pending_changes_settled_change_applies_to_lists_fetched_before_it_settled() {
        let mut pending = PendingChanges::default();
        let change = pending.start(removed_from_inbox(1));
        let list_fetch_before = pending.start_list_fetch();
        pending.settle(change, true);

        assert_eq!(
            pending.effects_on(list_fetch_before, NotificationSection::Inbox),
            vec![removed_from_inbox(1)]
        );
        assert!(!pending.is_running(notification_id(1)));

        let list_fetch_after = pending.start_list_fetch();
        assert!(
            pending
                .effects_on(list_fetch_after, NotificationSection::Inbox)
                .is_empty()
        );
        assert!(pending.changes.is_empty());
    }

    #[wasm_bindgen_test]
    fn test_pending_changes_failed_change_is_dropped() {
        let mut pending = PendingChanges::default();
        let change = pending.start(removed_from_inbox(1));
        let list_fetch = pending.start_list_fetch();
        pending.settle(change, false);

        assert!(
            pending
                .effects_on(list_fetch, NotificationSection::Inbox)
                .is_empty()
        );
        assert!(!pending.is_running(notification_id(1)));
    }

    #[wasm_bindgen_test]
    fn test_local_changes_skip_echo_of_running_changes() {
        let mut local_changes = LocalChanges::default();
        let change = local_changes.pending.start(removed_from_inbox(1));

        assert!(local_changes.is_own_change(Some(&[notification_id(1)]), 0.0));
        assert!(!local_changes.is_own_change(Some(&[notification_id(2)]), 0.0));

        local_changes.pending.settle(change, false);
        assert!(!local_changes.is_own_change(Some(&[notification_id(1)]), 0.0));
    }

    #[wasm_bindgen_test]
    fn test_local_changes_skip_events_without_ids_while_delete_all_runs() {
        let mut local_changes = LocalChanges::default();
        let change = local_changes
            .pending
            .start(PendingEffect::RemovedAll(NotificationSection::Inbox));

        assert!(local_changes.is_own_change(None, 0.0));

        local_changes.pending.settle(change, true);
        assert!(!local_changes.is_own_change(None, 0.0));
    }

    #[wasm_bindgen_test]
    fn test_enqueue_list_job_skips_counts_refresh_already_queued() {
        let mut list_jobs = VecDeque::new();
        enqueue_list_job(&mut list_jobs, ListJob::RefreshCounts);
        enqueue_list_job(&mut list_jobs, ListJob::RefreshCounts);
        enqueue_list_job(
            &mut list_jobs,
            ListJob::Refresh {
                refresh_counts: false,
            },
        );

        assert_eq!(
            list_jobs,
            VecDeque::from([
                ListJob::RefreshCounts,
                ListJob::Refresh {
                    refresh_counts: false
                },
            ])
        );
    }

    #[wasm_bindgen_test]
    fn test_local_changes_bulk_echo_seen_while_running_is_not_awaited_again() {
        let ids: Vec<NotificationId> = (0..=MAX_EVENT_IDS as u128).map(notification_id).collect();
        let mut local_changes = LocalChanges::default();
        let change = local_changes
            .pending
            .start(PendingEffect::RemovedAll(NotificationSection::Inbox));

        assert!(local_changes.is_own_change(None, 0.0));
        assert!(!local_changes.is_own_change(None, 0.0));

        local_changes.settle(change, true, &ids, 0.0);
        assert!(!local_changes.is_own_change(None, 1_000.0));
        assert!(local_changes.is_own_change(Some(&ids), 1_000.0));
    }

    #[wasm_bindgen_test]
    fn test_local_changes_bulk_echo_after_settle_is_skipped_once() {
        let ids: Vec<NotificationId> = (0..=MAX_EVENT_IDS as u128).map(notification_id).collect();
        let mut local_changes = LocalChanges::default();
        let change = local_changes
            .pending
            .start(PendingEffect::RemovedAll(NotificationSection::Inbox));

        local_changes.settle(change, true, &ids, 0.0);

        assert!(local_changes.is_own_change(None, 1_000.0));
        assert!(!local_changes.is_own_change(None, 1_000.0));
    }
}
