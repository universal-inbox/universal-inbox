#![allow(non_snake_case)]

use dioxus::prelude::*;
use fermi::use_atom_ref;
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

pub fn NotificationsPage(cx: Scope) -> Element {
    let notifications_page_ref = use_atom_ref(cx, &NOTIFICATIONS_PAGE);
    let ui_model_ref = use_atom_ref(cx, &UI_MODEL);
    let api_base_url = use_memo(cx, (), |()| get_api_base_url().unwrap());

    let task_service_integration_connection_ref =
        use_atom_ref(cx, &TASK_SERVICE_INTEGRATION_CONNECTION);

    let notification_service = use_coroutine_handle::<NotificationCommand>(cx).unwrap();

    let notification_to_plan: &UseState<Option<NotificationWithTask>> = use_state(cx, || None);
    let notification_to_link: &UseState<Option<NotificationWithTask>> = use_state(cx, || None);

    debug!("Rendering notifications page");

    use_future(cx, (), |()| {
        to_owned![notification_service];

        async move {
            notification_service.send(NotificationCommand::Refresh);
        }
    });

    let selected_notification = use_memo(
        cx,
        &(
            ui_model_ref.read().selected_notification_index,
            notifications_page_ref.read().clone(),
        ),
        |(selected_notification_index, notifications_page)| {
            let selected_notification = notifications_page.content.get(selected_notification_index);
            if let Some(notification) = selected_notification {
                notification_to_plan.set(Some(notification.clone()));
                notification_to_link.set(Some(notification.clone()));
            }
            selected_notification.cloned()
        },
    );

    render! {
        div {
            id: "notifications-page",
            class: "h-full mx-auto flex flex-row px-4 divide-x divide-base-200",

            if notifications_page_ref.read().content.is_empty() {
                render! {
                    div {
                        class: "relative w-full h-full flex justify-center items-center",
                        img {
                            class: "h-full opacity-30 dark:opacity-10",
                            src: "images/ui-logo-symbol-transparent.svg",
                            alt: "No notifications"
                        }
                        div {
                            class: "flex flex-col items-center absolute h-full object-center top-1/2 transform translate-y-1/4",
                            p { class: "text-gray-500 font-semibold", "Congrats! You have reached zero inbox 🎉" }
                            p { class: "text-gray-400", "You don't have any new notifications." }
                        }
                    }
                }
            } else {
                render! {
                    div {
                        id: "notifications-list",
                        class: "h-full basis-2/3 overflow-auto scroll-auto px-2 snap-y snap-mandatory",

                        NotificationsList {
                            notifications: notifications_page_ref.read().content.clone(),
                            ui_model_ref: ui_model_ref.clone(),
                            on_delete: |notification: &NotificationWithTask| {
                                notification_service.send(NotificationCommand::DeleteFromNotification(notification.clone()));
                            },
                            on_unsubscribe: |notification: &NotificationWithTask| {
                                notification_service.send(NotificationCommand::Unsubscribe(notification.id))
                            },
                            on_snooze: |notification: &NotificationWithTask| {
                                notification_service.send(NotificationCommand::Snooze(notification.id))
                            },
                            on_complete_task: |notification: &NotificationWithTask| {
                                notification_service.send(NotificationCommand::CompleteTaskFromNotification(notification.clone()));
                            },
                            on_plan: |notification: &NotificationWithTask| {
                                notification_to_plan.set(Some(notification.clone()));
                                ui_model_ref.write().task_planning_modal_opened = true;
                            },
                            on_link: |notification: &NotificationWithTask| {
                                notification_to_link.set(Some(notification.clone()));
                                ui_model_ref.write().task_link_modal_opened = true;
                            }
                        }
                    }

                    if let Some(ref notification) = selected_notification {
                        render! {
                            div {
                                class: "h-full basis-1/3 overflow-auto scroll-auto px-2 py-2 flex flex-row",

                                NotificationPreview {
                                    notification: notification,
                                    ui_model_ref: ui_model_ref.clone()
                                }
                            }
                        }
                    }
                }
            }
        }

        ui_model_ref.read().task_planning_modal_opened.then(|| {
            notification_to_plan.as_ref().map(|notification_to_plan| {
                render! {
                    TaskPlanningModal {
                        notification_to_plan: notification_to_plan.clone(),
                        task_service_integration_connection_ref: task_service_integration_connection_ref.clone(),
                        ui_model_ref: ui_model_ref.clone(),
                        on_close: |_| { ui_model_ref.write().task_planning_modal_opened = false; },
                        on_task_planning: |(params, task_id)| {
                            ui_model_ref.write().task_planning_modal_opened = false;
                            notification_service.send(NotificationCommand::PlanTask(
                                    notification_to_plan.clone(),
                                    task_id,
                                    params
                                ));
                        },
                        on_task_creation: |params| {
                            ui_model_ref.write().task_planning_modal_opened = false;
                            notification_service.send(NotificationCommand::CreateTaskFromNotification(
                                notification_to_plan.clone(),
                                params));
                        },
                    }
                }
            })
        })

        ui_model_ref.read().task_link_modal_opened.then(|| {
            notification_to_link.as_ref().map(|notification_to_link| {
                render! {
                    TaskLinkModal {
                        api_base_url: api_base_url.clone(),
                        notification_to_link: notification_to_link.clone(),
                        ui_model_ref: ui_model_ref.clone(),
                        on_close: |_| { ui_model_ref.write().task_link_modal_opened = false; },
                        on_task_link: |task_id| {
                            ui_model_ref.write().task_link_modal_opened = false;
                            notification_service.send(NotificationCommand::LinkNotificationWithTask(
                                notification_to_link.id,
                                task_id,
                            ));
                        },
                    }
                }
            })
        })
    }
}
