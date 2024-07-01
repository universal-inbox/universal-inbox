#![allow(non_snake_case)]

use dioxus::prelude::*;

use log::debug;

use universal_inbox::notification::NotificationWithTask;

use crate::{
    components::{
        notification_preview::NotificationPreview, notifications_list::NotificationsList,
        task_link_modal::TaskLinkModal, task_planning_modal::TaskPlanningModal,
    },
    config::get_api_base_url,
    model::UI_MODEL,
    services::{
        integration_connection_service::TASK_SERVICE_INTEGRATION_CONNECTION,
        notification_service::{NotificationCommand, NOTIFICATIONS_PAGE},
    },
};

pub fn NotificationsPage() -> Element {
    let api_base_url = use_memo(move || get_api_base_url().unwrap());

    let notification_service = use_coroutine_handle::<NotificationCommand>();

    let mut notification_to_plan: Signal<Option<NotificationWithTask>> = use_signal(|| None);
    let mut notification_to_link: Signal<Option<NotificationWithTask>> = use_signal(|| None);

    debug!("Rendering notifications page");

    let selected_notification = use_memo(move || {
        let notifications_page = NOTIFICATIONS_PAGE();
        let selected_notification = notifications_page
            .content
            .get(UI_MODEL.read().selected_notification_index);
        if let Some(notification) = selected_notification {
            *notification_to_plan.write() = Some(notification.clone());
            *notification_to_link.write() = Some(notification.clone());
        }
        selected_notification.cloned()
    });

    rsx! {
        div {
            id: "notifications-page",
            class: "h-full mx-auto flex flex-row px-4 divide-x divide-base-200",

            if NOTIFICATIONS_PAGE.read().content.is_empty() {
                div {
                    class: "relative w-full h-full flex justify-center items-center",
                    img {
                        class: "h-full opacity-30 dark:opacity-10",
                        src: "images/ui-logo-symbol-transparent.svg",
                        alt: "No notifications"
                    }
                    div {
                        class: "relative w-full h-full flex justify-center items-center",
                        img {
                            class: "h-full opacity-30 dark:opacity-10",
                            src: "images/ui-logo-symbol-transparent.svg",
                            alt: "No notifications"
                        }
                        div {
                            class: "flex flex-col items-center absolute object-center top-2/3 transform translate-y-1/4",
                            p { class: "text-gray-500 font-semibold", "Congrats! You have reached zero inbox 🎉" }
                            p { class: "text-gray-400", "You don't have any new notifications." }
                        }
                    }
                }
            } else {
                div {
                    id: "notifications-list",
                    class: "h-full basis-2/3 overflow-auto scroll-auto px-2 snap-y snap-mandatory",

                    NotificationsList {
                        notifications: NOTIFICATIONS_PAGE.read().content.clone(),
                        ui_model: UI_MODEL.signal(),
                        on_delete: move |notification: NotificationWithTask| {
                            notification_service.send(NotificationCommand::DeleteFromNotification(notification.clone()));
                        },
                        on_unsubscribe: move |notification: NotificationWithTask| {
                            notification_service.send(NotificationCommand::Unsubscribe(notification.id))
                        },
                        on_snooze: move |notification: NotificationWithTask| {
                            notification_service.send(NotificationCommand::Snooze(notification.id))
                        },
                        on_complete_task: move |notification: NotificationWithTask| {
                            notification_service.send(NotificationCommand::CompleteTaskFromNotification(notification.clone()));
                        },
                        on_plan: move |notification: NotificationWithTask| {
                            *notification_to_plan.write() = Some(notification.clone());
                            UI_MODEL.write().task_planning_modal_opened = true;
                        },
                        on_link: move |notification: NotificationWithTask| {
                            *notification_to_link.write() = Some(notification.clone());
                            UI_MODEL.write().task_link_modal_opened = true;
                        }
                    }
                }

                if let Some(notification) = selected_notification() {
                    div {
                        class: "h-full basis-1/3 overflow-auto scroll-auto px-2 py-2 flex flex-row",

                        NotificationPreview {
                            notification: notification,
                            ui_model: UI_MODEL.signal()
                        }
                    }
                }
            }
        }

        if UI_MODEL.read().task_planning_modal_opened {
            if let Some(notification_to_plan) = notification_to_plan().map(Signal::new) {
                TaskPlanningModal {
                    notification_to_plan: notification_to_plan,
                    task_service_integration_connection: TASK_SERVICE_INTEGRATION_CONNECTION.signal(),
                    ui_model: UI_MODEL.signal(),
                    on_close: move |_| { UI_MODEL.write().task_planning_modal_opened = false; },
                    on_task_planning: move |(params, task_id)| {
                        UI_MODEL.write().task_planning_modal_opened = false;
                        notification_service.send(NotificationCommand::PlanTask(
                            notification_to_plan(),
                            task_id,
                            params
                        ));
                    },
                    on_task_creation: move |params| {
                        UI_MODEL.write().task_planning_modal_opened = false;
                        notification_service.send(NotificationCommand::CreateTaskFromNotification(
                            notification_to_plan(),
                            params));
                    },
                }
            }
        }

        if UI_MODEL.read().task_link_modal_opened {
            if let Some(notification_to_link) = notification_to_link().map(Signal::new) {
                TaskLinkModal {
                    api_base_url: api_base_url,
                    notification_to_link: notification_to_link,
                    ui_model: UI_MODEL.signal(),
                    on_close: move |_| { UI_MODEL.write().task_link_modal_opened = false; },
                    on_task_link: move |task_id| {
                        UI_MODEL.write().task_link_modal_opened = false;
                        notification_service.send(NotificationCommand::LinkNotificationWithTask(
                            notification_to_link().id,
                            task_id,
                        ));
                    },
                }
            }
        }
    }
}
