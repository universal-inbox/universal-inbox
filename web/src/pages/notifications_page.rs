#![allow(non_snake_case)]

use dioxus::prelude::*;
use log::debug;
use web_sys::KeyboardEvent;

use universal_inbox::{notification::NotificationWithTask, HasHtmlUrl, Page};

use crate::{
    components::{
        notification_preview::NotificationPreview, notifications_list::NotificationsList,
    },
    icons::UILogo,
    keyboard_manager::{KeyboardHandler, KEYBOARD_MANAGER},
    model::{PreviewPane, UI_MODEL},
    services::{
        flyonui::{has_flyonui_modal_opened, open_flyonui_modal},
        notification_service::{NotificationCommand, NOTIFICATIONS_PAGE, NOTIFICATION_FILTERS},
    },
    utils::{get_screen_width, open_link, scroll_element, scroll_element_by_page},
};

static KEYBOARD_HANDLER: NotificationsPageKeyboardHandler = NotificationsPageKeyboardHandler {};

pub fn NotificationsPage() -> Element {
    debug!("Rendering notifications page");
    let notifications =
        Into::<ReadOnlySignal<Page<NotificationWithTask>>>::into(NOTIFICATIONS_PAGE.signal());

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

    use_drop(move || {
        KEYBOARD_MANAGER.write().active_keyboard_handler = None;
    });

    rsx! {
        div {
            id: "notifications-page",
            class: "h-full mx-auto flex flex-row lg:px-4 lg:divide-x divide-base-content/25 relative",
            onmounted: move |_| {
                KEYBOARD_MANAGER.write().active_keyboard_handler = Some(&KEYBOARD_HANDLER);
            },

            if NOTIFICATIONS_PAGE.read().content.is_empty() && !NOTIFICATION_FILTERS().is_filtered() {
                div {
                    class: "relative w-full h-full flex flex-col justify-center items-center",
                    UILogo {
                        class: "opacity-30 dark:opacity-10 w-96 h-96",
                        alt: "No notifications"
                    }
                    div {
                        class: "flex flex-col items-center",
                        p { class: "text-gray-500 font-semibold", "Congrats! You have reached inbox zero 🎉" }
                        p { class: "text-base-content/50", "You don't have any new notifications." }
                    }
                }
            } else {
                div {
                    class: "h-full lg:basis-2/3 max-lg:w-full max-lg:absolute",

                    NotificationsList {
                        notifications,
                        notification_filters: NOTIFICATION_FILTERS.signal(),
                    }
                }

                if let Some(index) = UI_MODEL.read().selected_notification_index {
                    if let Some(notification) = NOTIFICATIONS_PAGE().content.get(index) {
                        div {
                            id: "notification-preview",
                            class: "h-full lg:basis-1/3 max-lg:w-full max-lg:absolute lg:max-w-sm xl:max-w-md 2xl:max-w-xl px-2 py-2 flex flex-row bg-base-100 z-auto",

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
        if has_flyonui_modal_opened() {
            return false;
        }
        let notification_service = use_coroutine_handle::<NotificationCommand>();
        let notifications_page = NOTIFICATIONS_PAGE();
        let list_length = notifications_page.content.len();
        let selected_notification_index = UI_MODEL.peek().selected_notification_index;
        let selected_notification =
            selected_notification_index.and_then(|index| notifications_page.content.get(index));
        let mut handled = true;

        match event.key().as_ref() {
            "ArrowDown" => {
                if let Some(index) = selected_notification_index {
                    if index < (list_length - 1) {
                        let mut ui_model = UI_MODEL.write();
                        ui_model.selected_notification_index = Some(index + 1);
                    }
                }
            }
            "ArrowUp" => {
                if let Some(index) = selected_notification_index {
                    if index > 0 {
                        let mut ui_model = UI_MODEL.write();
                        ui_model.selected_notification_index = Some(index - 1);
                    }
                }
            }
            "ArrowRight"
                if UI_MODEL.peek().selected_preview_pane == PreviewPane::Notification
                    && selected_notification
                        .map(|notif| notif.task.is_some())
                        .unwrap_or_default() =>
            {
                UI_MODEL.write().selected_preview_pane = PreviewPane::Task;
            }
            "ArrowLeft"
                if UI_MODEL.peek().selected_preview_pane == PreviewPane::Task
                    && !selected_notification
                        .map(|notif| notif.is_built_from_task())
                        .unwrap_or_default() =>
            {
                UI_MODEL.write().selected_preview_pane = PreviewPane::Notification;
            }
            "d" => {
                if let Some(notification) = selected_notification {
                    notification_service.send(NotificationCommand::DeleteFromNotification(
                        notification.clone(),
                    ))
                }
            }
            "c" => {
                if let Some(notification) = selected_notification {
                    notification_service.send(NotificationCommand::CompleteTaskFromNotification(
                        notification.clone(),
                    ))
                }
            }
            "u" => {
                if let Some(notification) = selected_notification {
                    notification_service.send(NotificationCommand::Unsubscribe(notification.id))
                }
            }
            "s" => {
                if let Some(notification) = selected_notification {
                    notification_service.send(NotificationCommand::Snooze(notification.id))
                }
            }
            "y" => {
                if let Some(notification) = selected_notification {
                    notification_service
                        .send(NotificationCommand::AcceptInvitation(notification.id))
                }
            }
            "n" => {
                if let Some(notification) = selected_notification {
                    notification_service
                        .send(NotificationCommand::DeclineInvitation(notification.id))
                }
            }
            "m" => {
                if let Some(notification) = selected_notification {
                    notification_service.send(NotificationCommand::TentativelyAcceptInvitation(
                        notification.id,
                    ))
                }
            }
            "p" => {
                if UI_MODEL.peek().is_task_actions_enabled {
                    open_flyonui_modal("#task-planning-modal");
                }
            }
            "l" => {
                if UI_MODEL.peek().is_task_actions_enabled {
                    open_flyonui_modal("#task-linking-modal");
                }
            }
            "j" => {
                let _ = scroll_element("notification-preview-details", 100.0);
            }
            "k" => {
                let _ = scroll_element("notification-preview-details", -100.0);
            }
            " " => {
                let _ = scroll_element_by_page("notification-preview-details");
            }
            "e" => {
                UI_MODEL.write().toggle_preview_cards();
            }
            "Enter" => {
                if let Some(notification) = selected_notification {
                    let _ = open_link(notification.get_html_url().as_str());
                }
            }
            "h" | "?" => UI_MODEL.write().toggle_help(),
            _ => handled = false,
        }

        handled
    }
}
