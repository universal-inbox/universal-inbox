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
        notification_service::{NOTIFICATION_FILTERS, NOTIFICATIONS_PAGE, NotificationCommand},
    },
    utils::{
        get_screen_width, open_link, scroll_element, scroll_element_by_page,
        scroll_element_into_view_by_class,
    },
};

static KEYBOARD_HANDLER: NotificationsPageKeyboardHandler = NotificationsPageKeyboardHandler {};

#[component]
pub fn NotificationPage(notification_id: NotificationId) -> Element {
    rsx! { InternalNotificationPage { notification_id } }
}

#[component]
pub fn NotificationsPage() -> Element {
    use_effect(move || {
        let notifications_count = NOTIFICATIONS_PAGE().content.len();
        if notifications_count > 0 {
            let mut model = UI_MODEL.write();
            if let Some(index) = model.selected_notification_index {
                if index >= notifications_count {
                    model.selected_notification_index = Some(notifications_count - 1);
                }
            } else if get_screen_width().unwrap_or_default() >= 1024 {
                // ie. lg screen
                model.selected_notification_index = Some(0);
            }
        }
    });

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

    // state → URL: push the URL when the user explicitly changes the selected index
    // (click, keyboard, prev/next).
    //
    // Two safeguards prevent this effect from racing with the URL→state effect above
    // on list refreshes — which would flip-flop the URL between two notifications and
    // freeze the app:
    //
    // 1. The `use_memo` deduplicates `selected_notification_index` changes so this
    //    effect does NOT re-fire on every `UI_MODEL.write()` (the API layer writes
    //    `authentication_state` on every request, which would otherwise re-trigger us).
    // 2. `notifications.peek()` reads the list without subscribing, so this effect
    //    does NOT re-fire on list refreshes/reorders.
    //
    // Together: this effect runs ONLY when the user actually changes the selected
    // index (or on initial mount).
    let selected_index = use_memo(move || UI_MODEL.read().selected_notification_index);
    use_effect(move || {
        let index = selected_index();
        if let Some(index) = index {
            if let Some(selected_notification) = notifications.peek().content.get(index)
                && *notification_id.peek() != Some(selected_notification.id)
            {
                let route = Route::NotificationPage {
                    notification_id: selected_notification.id,
                };
                nav.push(route);
            }
        } else if notification_id.peek().is_some() {
            nav.push(Route::NotificationsPage {});
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

            if NOTIFICATIONS_PAGE.read().content.is_empty() && !NOTIFICATION_FILTERS().is_filtered() {
                WelcomeHero { inbox_zero_message: "Your notifications will appear here when they arrive." }
            } else {
                NotificationsList {
                    notifications,
                    notification_filters: NOTIFICATION_FILTERS.signal(),
                }

                if let Some(index) = UI_MODEL.read().selected_notification_index {
                    if let Some(notification) = NOTIFICATIONS_PAGE().content.get(index) {
                        div {
                            // `.detail-panel` class hook preserved for the
                            // shell CSS (flex: 1, bg, overflow). Mobile
                            // visibility is now driven by `max-md:hidden!`
                            // (default off) + `max-md:[.app-layout.show-detail_&]:flex!`
                            // (revealed when the parent layout enters
                            // show-detail at ≤768px). Desktop renders both
                            // panes side-by-side regardless. `!important`
                            // (`!` suffix) is required to win against the
                            // `.detail-panel { display: flex }` cascade.
                            class: "detail-panel max-md:hidden! max-md:[.app-layout.show-detail_&]:flex! max-md:[.app-layout.show-detail_&]:flex-1!",
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
                    notification_service.send(NotificationCommand::DeleteFromNotification(
                        notification.clone(),
                    ))
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
                    notification_service.send(NotificationCommand::Snooze(notification.id))
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
                    let _ = open_link(notification.get_html_url().as_str());
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
