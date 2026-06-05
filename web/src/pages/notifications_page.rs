#![allow(non_snake_case)]

use dioxus::prelude::dioxus_core::use_drop;
use dioxus::prelude::*;
use log::debug;
use web_sys::KeyboardEvent;

use universal_inbox::{
    HasHtmlUrl, Page,
    notification::{NotificationId, NotificationStatus, NotificationWithTask},
};

use crate::{
    components::{
        notification_preview::NotificationPreview, notifications_list::NotificationsList,
        welcome_hero::WelcomeHero,
    },
    keyboard_manager::{KEYBOARD_MANAGER, KeyboardHandler},
    model::{PreviewPane, UI_MODEL},
    route::Route,
    services::{
        flyonui::open_flyonui_modal,
        notification_service::{
            CURRENT_NOTIFICATION_SECTION, NOTIFICATION_FILTERS, NOTIFICATIONS_PAGE,
            NotificationCommand, NotificationSection,
        },
        user_preferences_service::USER_PREFERENCES,
    },
    utils::{
        get_screen_width, open_link, open_link_in_background, scroll_element,
        scroll_element_by_page, scroll_element_into_view_by_class,
    },
};

static KEYBOARD_HANDLER: NotificationsPageKeyboardHandler = NotificationsPageKeyboardHandler {};

#[component]
pub fn NotificationPage(notification_id: NotificationId) -> Element {
    rsx! { InternalNotificationPage { notification_id } }
}

/// On entering a section's list, switch the active section and refresh the list — but
/// only when the section actually changes. The inbox is already loaded by the periodic
/// refresh in the authenticated layout, so re-entering the same section issues no
/// redundant refresh (which would add render churn during navigation). Runs once per
/// mount.
fn enter_notification_section(section: NotificationSection) {
    let notification_service = use_coroutine_handle::<NotificationCommand>();
    use_hook(move || {
        if *CURRENT_NOTIFICATION_SECTION.peek() != section {
            *CURRENT_NOTIFICATION_SECTION.write() = section;
            UI_MODEL.write().selected_notification_index = None;
            notification_service.send(NotificationCommand::Refresh);
        }
    });
}

#[component]
pub fn NotificationsPage() -> Element {
    enter_notification_section(NotificationSection::Inbox);
    rsx! { InternalNotificationPage {} }
}

/// The list route for a section — used to push the URL back to the list when the
/// selection is cleared.
fn section_list_route(section: NotificationSection) -> Route {
    match section {
        NotificationSection::Inbox => Route::NotificationsPage {},
        NotificationSection::Snoozed => Route::SnoozedNotificationsPage {},
        NotificationSection::Deleted => Route::DeletedNotificationsPage {},
    }
}

#[component]
pub fn SnoozedNotificationsPage() -> Element {
    enter_notification_section(NotificationSection::Snoozed);
    rsx! { InternalNotificationPage {} }
}

#[component]
pub fn DeletedNotificationsPage() -> Element {
    enter_notification_section(NotificationSection::Deleted);
    rsx! { InternalNotificationPage {} }
}

#[component]
fn InternalNotificationPage(notification_id: ReadSignal<Option<NotificationId>>) -> Element {
    let notifications =
        Into::<ReadSignal<Page<NotificationWithTask>>>::into(NOTIFICATIONS_PAGE.signal());
    let nav = use_navigator();
    debug!(
        "Rendering notifications page for notification {:?}",
        notification_id()
    );

    let notification_service = use_coroutine_handle::<NotificationCommand>();

    // Keep the selection valid as the list changes. Clamping runs on any route so that
    // deleting/snoozing the last notification lands on the new last one (instead of an
    // out-of-range index). Auto-selecting the first notification only happens in the
    // list view (no id in the URL) and on large screens, so it never hijacks a deep link.
    use_effect(move || {
        let notifications_count = notifications().content.len();
        if notifications_count == 0 {
            return;
        }
        let has_url_id = notification_id().is_some();
        let mut model = UI_MODEL.write();
        if let Some(index) = model.selected_notification_index {
            if index >= notifications_count {
                model.selected_notification_index = Some(notifications_count - 1);
            }
        } else if !has_url_id && get_screen_width().unwrap_or_default() >= 1024 {
            // ie. lg screen
            model.selected_notification_index = Some(0);
        }
    });

    // Deep link resolution. Keyed on the URL id ONLY (via the memo) so it fires on
    // genuine navigation to a notification — not when the list mutates. This matters
    // after an optimistic delete/snooze: the URL still points at the removed
    // notification, and re-resolving it would wrongly switch sections and re-add it.
    use_effect(move || {
        // Tracks `notification_id()` only; the list is read via `peek()` so this does
        // NOT re-fire when the list mutates.
        if let Some(id) = notification_id()
            && !NOTIFICATIONS_PAGE
                .peek()
                .content
                .iter()
                .any(|notif| notif.id == id)
        {
            notification_service.send(NotificationCommand::LoadAndSelect(id));
        }
    });

    // URL → state: reconcile the selected index from the URL and the current list.
    use_effect(move || {
        if let Some(notification_id) = notification_id() {
            if let Some(notification_index) = notifications()
                .content
                .iter()
                .position(|n| n.id == notification_id)
                && UI_MODEL.peek().selected_notification_index != Some(notification_index)
            {
                UI_MODEL.write().selected_notification_index = Some(notification_index);
            }
        } else if UI_MODEL.peek().selected_notification_index.is_some()
            && get_screen_width().unwrap_or_default() < 1024
        {
            UI_MODEL.write().selected_notification_index = None;
        }
    });

    // state → URL: push the URL to track the *identity* of the selected notification,
    // not just its index. Keying on the resolved id (index → notification) means the
    // URL follows the selection when:
    //   - the user changes the selection (click / keyboard),
    //   - an action (delete / snooze / unsubscribe) removes the current notification
    //     and the next one slides into the same index,
    //   - switching sections reloads a different list under the same index.
    //
    // The `use_memo` resolves the id and deduplicates: unrelated `UI_MODEL` writes (the
    // API layer stamps `authentication_state` on every request) and list refreshes that
    // don't change the selected id do NOT re-fire this effect — preventing the URL
    // flip-flop the previous index-only version guarded against.
    let selected_notification_id = use_memo(move || {
        UI_MODEL
            .read()
            .selected_notification_index
            .and_then(|index| notifications().content.get(index).map(|notif| notif.id))
    });
    use_effect(move || {
        if let Some(id) = selected_notification_id() {
            if *notification_id.peek() != Some(id) {
                nav.push(Route::NotificationPage {
                    notification_id: id,
                });
            }
        } else if notification_id.peek().is_some() {
            nav.push(section_list_route(*CURRENT_NOTIFICATION_SECTION.peek()));
        }
    });

    use_drop(move || {
        KEYBOARD_MANAGER.write().active_keyboard_handler = None;
    });

    rsx! {
        div {
            id: "notifications-page",
            class: "flex h-full",
            onmounted: move |_| {
                KEYBOARD_MANAGER.write().active_keyboard_handler = Some(&KEYBOARD_HANDLER);
            },

            if CURRENT_NOTIFICATION_SECTION() == NotificationSection::Inbox
                && NOTIFICATIONS_PAGE.read().content.is_empty()
                && !NOTIFICATION_FILTERS().is_filtered()
            {
                WelcomeHero { inbox_zero_message: "Your notifications will appear here when they arrive." }
            } else {
                NotificationsList {
                    notifications,
                    notification_filters: NOTIFICATION_FILTERS.signal(),
                }

                if let Some(index) = UI_MODEL.read().selected_notification_index {
                    if let Some(notification) = NOTIFICATIONS_PAGE().content.get(index) {
                        div {
                            // Shell utilities (flex-1, bg, overflow) live inline. Mobile
                            // visibility is driven by `max-md:hidden!` (default off) +
                            // `max-md:[.app-layout.show-detail_&]:flex!` (revealed when
                            // the parent layout enters show-detail at ≤768px). Desktop
                            // renders both panes side-by-side regardless. `!important`
                            // (`!` suffix) wins against the baseline `flex` so the
                            // responsive `hidden!`/`flex!` toggle is decisive.
                            class: "flex-1 flex flex-col bg-ui-base-200 overflow-hidden min-w-0 max-md:hidden! max-md:[.app-layout.show-detail_&]:flex! max-md:[.app-layout.show-detail_&]:flex-1!",
                            NotificationPreview {
                                notification: notification.clone(),
                                notifications_count: notifications().content.len(),
                                ui_model: UI_MODEL.signal()
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(PartialEq)]
struct NotificationsPageKeyboardHandler {}

impl KeyboardHandler for NotificationsPageKeyboardHandler {
    fn handle_keydown(&self, event: &KeyboardEvent) -> bool {
        let notification_service = use_coroutine_handle::<NotificationCommand>();
        let notifications_page = NOTIFICATIONS_PAGE();
        let list_length = notifications_page.content.len();
        let selected_notification_index = UI_MODEL.peek().selected_notification_index;
        let selected_notification =
            selected_notification_index.and_then(|index| notifications_page.content.get(index));
        let mut handled = true;

        match (
            event.key().as_ref(),
            event.ctrl_key(),
            event.meta_key(),
            event.alt_key(),
            event.shift_key(),
        ) {
            ("ArrowDown", false, false, false, false) => {
                if let Some(index) = selected_notification_index
                    && index < (list_length - 1)
                {
                    let new_index = index + 1;
                    let mut ui_model = UI_MODEL.write();
                    ui_model.selected_notification_index = Some(new_index);
                    drop(ui_model);
                    if let Some(notif) = notifications_page.content.get(new_index)
                        && notif.status == NotificationStatus::Unread
                    {
                        notification_service.send(NotificationCommand::MarkAsRead(notif.id));
                    }
                    let _ = scroll_element_into_view_by_class(
                        "notifications-list",
                        "row-hover",
                        new_index,
                    );
                }
            }
            ("ArrowUp", false, false, false, false) => {
                if let Some(index) = selected_notification_index
                    && index > 0
                {
                    let new_index = index - 1;
                    let mut ui_model = UI_MODEL.write();
                    ui_model.selected_notification_index = Some(new_index);
                    drop(ui_model);
                    if let Some(notif) = notifications_page.content.get(new_index)
                        && notif.status == NotificationStatus::Unread
                    {
                        notification_service.send(NotificationCommand::MarkAsRead(notif.id));
                    }
                    let _ = scroll_element_into_view_by_class(
                        "notifications-list",
                        "row-hover",
                        new_index,
                    );
                }
            }
            ("ArrowRight", false, false, false, false)
                if UI_MODEL.peek().selected_preview_pane == PreviewPane::Notification
                    && selected_notification
                        .map(|notif| notif.task.is_some())
                        .unwrap_or_default() =>
            {
                UI_MODEL.write().selected_preview_pane = PreviewPane::Task;
            }
            ("ArrowLeft", false, false, false, false)
                if UI_MODEL.peek().selected_preview_pane == PreviewPane::Task
                    && !selected_notification
                        .map(|notif| notif.is_built_from_task())
                        .unwrap_or_default() =>
            {
                UI_MODEL.write().selected_preview_pane = PreviewPane::Notification;
            }
            ("d", false, false, false, false) => {
                if let Some(notification) = selected_notification {
                    if CURRENT_NOTIFICATION_SECTION() == NotificationSection::Deleted {
                        // Undelete replaces Delete in the Deleted section; task-built
                        // notifications cannot be restored (backend restriction).
                        if !notification.is_built_from_task() {
                            notification_service
                                .send(NotificationCommand::Undelete(notification.id))
                        }
                    } else {
                        notification_service.send(NotificationCommand::DeleteFromNotification(
                            notification.clone(),
                        ))
                    }
                }
            }
            ("c", false, false, false, false) => {
                if let Some(notification) = selected_notification {
                    notification_service.send(NotificationCommand::CompleteTaskFromNotification(
                        notification.clone(),
                    ))
                }
            }
            ("u", false, false, false, false) => {
                if let Some(notification) = selected_notification {
                    notification_service.send(NotificationCommand::Unsubscribe(notification.id))
                }
            }
            ("s", false, false, false, false) => {
                if let Some(notification) = selected_notification {
                    if CURRENT_NOTIFICATION_SECTION() == NotificationSection::Snoozed {
                        notification_service.send(NotificationCommand::Unsnooze(notification.id))
                    } else {
                        notification_service.send(NotificationCommand::Snooze(notification.id))
                    }
                }
            }
            ("t", false, false, false, false) => {
                if let Some(notification) = selected_notification {
                    notification_service.send(
                        NotificationCommand::CreateTaskWithDetaultsFromNotification(
                            notification.clone(),
                        ),
                    )
                }
            }
            ("y", false, false, false, false) => {
                if let Some(notification) = selected_notification {
                    notification_service
                        .send(NotificationCommand::AcceptInvitation(notification.id))
                }
            }
            ("n", false, false, false, false) => {
                if let Some(notification) = selected_notification {
                    notification_service
                        .send(NotificationCommand::DeclineInvitation(notification.id))
                }
            }
            ("m", false, false, false, false) => {
                if let Some(notification) = selected_notification {
                    notification_service.send(NotificationCommand::TentativelyAcceptInvitation(
                        notification.id,
                    ))
                }
            }
            ("p", false, false, false, false) => {
                if UI_MODEL.peek().is_task_actions_enabled {
                    open_flyonui_modal("#task-planning-modal");
                }
            }
            ("l", false, false, false, false) => {
                if UI_MODEL.peek().is_task_actions_enabled {
                    open_flyonui_modal("#task-linking-modal");
                }
            }
            ("j", false, false, false, false) => {
                let _ = scroll_element("notification-preview-details", 100.0);
            }
            ("k", false, false, false, false) => {
                let _ = scroll_element("notification-preview-details", -100.0);
            }
            (" ", false, false, false, false) => {
                let _ = scroll_element_by_page("notification-preview-details");
            }
            ("e", false, false, false, false) => {
                UI_MODEL.write().toggle_preview_cards();
            }
            ("Enter", false, false, false, false) => {
                if let Some(notification) = selected_notification {
                    let url = notification.get_html_url();
                    let open_in_background = USER_PREFERENCES
                        .peek()
                        .as_ref()
                        .map(|prefs| prefs.open_links_in_background)
                        .unwrap_or(false);
                    let _ = if open_in_background {
                        open_link_in_background(url.as_str())
                    } else {
                        open_link(url.as_str())
                    };
                }
            }
            ("h", false, false, false, false) | ("?", false, false, false, false) => {
                UI_MODEL.write().toggle_help()
            }
            _ => handled = false,
        }

        handled
    }
}
