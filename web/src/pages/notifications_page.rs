use log::debug;

use dioxus::prelude::*;
use fermi::use_atom_ref;

use universal_inbox::notification::NotificationWithTask;

use crate::{
    components::{
        notifications_list::notifications_list, task_association_modal::task_association_modal,
        task_planning_modal::task_planning_modal,
    },
    config::get_api_base_url,
    model::UI_MODEL,
    services::notification_service::{NotificationCommand, NOTIFICATIONS},
};

pub fn notifications_page(cx: Scope) -> Element {
    let notifications_ref = use_atom_ref(cx, NOTIFICATIONS);
    let ui_model_ref = use_atom_ref(cx, UI_MODEL);
    let api_base_url = use_memo(cx, (), |()| get_api_base_url().unwrap());

    let notification_service = use_coroutine_handle::<NotificationCommand>(cx).unwrap();

    let notification_to_plan: &UseState<Option<NotificationWithTask>> = use_state(cx, || None);
    let notification_to_associate: &UseState<Option<NotificationWithTask>> = use_state(cx, || None);

    debug!("Rendering notifications page");
    use_memo(
        cx,
        &(
            ui_model_ref.read().selected_notification_index,
            notifications_ref.read().clone(),
        ),
        |(selected_notification_index, notifications)| {
            if let Some(notification) = notifications.get(selected_notification_index) {
                notification_to_plan.set(Some(notification.clone()));
                notification_to_associate.set(Some(notification.clone()));
            }
        },
    );

    cx.render(rsx!(
        div {
            id: "notifications-page",
            class: "w-full h-full flex-1 overflow-auto snap-y snap-mandatory",

            div {
                class: "container h-full mx-auto",

                if notifications_ref.read().is_empty() {
                    rsx!(
                        img {
                            class: "w-screen h-full object-contain object-center object-top opacity-30 dark:opacity-10",
                            src: "images/ui-logo-symbol-transparent.svg",
                            alt: "No notifications"
                        }
                    )
                } else {
                    rsx!(
                        self::notifications_list {
                            notifications: notifications_ref.read().clone(),
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
                            on_associate: |notification: &NotificationWithTask| {
                                notification_to_associate.set(Some(notification.clone()));
                                ui_model_ref.write().task_association_modal_opened = true;
                            }
                        }
                    )
                }
            }
        }

        ui_model_ref.read().task_planning_modal_opened.then(|| {
            notification_to_plan.as_ref().map(|notification_to_plan| {
                rsx!{
                    task_planning_modal {
                        notification_to_plan: notification_to_plan.clone(),
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

        ui_model_ref.read().task_association_modal_opened.then(|| {
            notification_to_associate.as_ref().map(|notification_to_associate| {
                rsx!{
                    task_association_modal {
                        api_base_url: api_base_url.clone(),
                        notification_to_associate: notification_to_associate.clone(),
                        ui_model_ref: ui_model_ref.clone(),
                        on_close: |_| { ui_model_ref.write().task_association_modal_opened = false; },
                        on_task_association: |task_id| {
                            ui_model_ref.write().task_association_modal_opened = false;
                            notification_service.send(NotificationCommand::AssociateNotificationWithTask(
                                notification_to_associate.id,
                                task_id,
                            ));
                        },
                    }
                }
            })
        })
    ))
}
