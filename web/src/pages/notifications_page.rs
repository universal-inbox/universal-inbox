#![allow(non_snake_case)]

use dioxus::prelude::*;

use log::debug;
use web_sys::KeyboardEvent;

use universal_inbox::HasHtmlUrl;

use crate::{
    components::{
        notification_preview::NotificationPreview, notifications_list::NotificationsList,
    },
    images::UI_LOGO_SYMBOL_TRANSPARENT,
    keyboard_manager::{KeyboardHandler, KEYBOARD_MANAGER},
    model::{PreviewPane, UI_MODEL},
    services::notification_service::{NotificationCommand, NOTIFICATIONS_PAGE},
    utils::{open_link, scroll_element, scroll_element_by_page},
};

static KEYBOARD_HANDLER: NotificationsPageKeyboardHandler = NotificationsPageKeyboardHandler {};

pub fn NotificationsPage() -> Element {
    debug!("Rendering notifications page");

    use_effect(move || {
        let notifications_count = NOTIFICATIONS_PAGE().content.len();
        if notifications_count > 0
            && UI_MODEL.read().selected_notification_index >= notifications_count
        {
            UI_MODEL.write().selected_notification_index = notifications_count - 1;
        }
    });

    use_drop(move || {
        KEYBOARD_MANAGER.write().active_keyboard_handler = None;
    });

    rsx! {
        div {
            id: "notifications-page",
            class: "h-full mx-auto flex flex-row px-4 divide-x divide-base-200",
            onmounted: move |_| {
                KEYBOARD_MANAGER.write().active_keyboard_handler = Some(&KEYBOARD_HANDLER);
            },

            if NOTIFICATIONS_PAGE.read().content.is_empty() {
                div {
                    class: "relative w-full h-full flex justify-center items-center",
                    img {
                        class: "h-full opacity-30 dark:opacity-10",
                        src: "{UI_LOGO_SYMBOL_TRANSPARENT}",
                        alt: "No notifications"
                    }
                    div {
                        class: "flex flex-col items-center absolute object-center top-2/3 transform translate-y-1/4",
                        p { class: "text-gray-500 font-semibold", "Congrats! You have reached inbox zero 🎉" }
                        p { class: "text-gray-400", "You don't have any new notifications." }
                    }
                }
            } else {
                div {
                    id: "notifications-list",
                    class: "h-full basis-2/3 overflow-auto scroll-auto px-2 snap-y snap-mandatory",

                    NotificationsList {
                        notifications: NOTIFICATIONS_PAGE.read().content.clone(),
                    }
                }

                if let Some(notification) = NOTIFICATIONS_PAGE()
                    .content.get(UI_MODEL.read().selected_notification_index) {
                    div {
                        id: "notification-preview",
                        class: "h-full basis-1/3 overflow-auto scroll-auto px-2 py-2 flex flex-row",

                        NotificationPreview {
                            notification: notification.clone(),
                            ui_model: UI_MODEL.signal()
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
        if UI_MODEL.peek().task_planning_modal_opened || UI_MODEL.peek().task_link_modal_opened {
            return match event.key().as_ref() {
                "Escape" => {
                    UI_MODEL.write().task_planning_modal_opened = false;
                    UI_MODEL.write().task_link_modal_opened = false;
                    true
                }
                _ => false,
            };
        }

        let notification_service = use_coroutine_handle::<NotificationCommand>();
        let notifications_page = NOTIFICATIONS_PAGE();
        let list_length = notifications_page.content.len();
        let selected_notification = notifications_page
            .content
            .get(UI_MODEL.peek().selected_notification_index);
        let mut handled = true;

        match event.key().as_ref() {
            "ArrowDown" if UI_MODEL.peek().selected_notification_index < (list_length - 1) => {
                let mut ui_model = UI_MODEL.write();
                ui_model.selected_notification_index += 1;
            }
            "ArrowUp" if UI_MODEL.peek().selected_notification_index > 0 => {
                let mut ui_model = UI_MODEL.write();
                ui_model.selected_notification_index -= 1;
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
            "p" => UI_MODEL.write().task_planning_modal_opened = true,
            "l" => UI_MODEL.write().task_link_modal_opened = true,
            "j" => {
                let _ = scroll_element("notification-preview", 100.0);
            }
            "k" => {
                let _ = scroll_element("notification-preview", -100.0);
            }
            " " => {
                let _ = scroll_element_by_page("notification-preview");
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
