use dioxus::prelude::*;
use fermi::use_atom_ref;

use universal_inbox::{notification::Notification, task::Task};

use crate::{
    components::{notifications_list::notifications_list, task_planning::task_planning_modal},
    services::{
        notification_service::{NotificationCommand, NOTIFICATIONS, UI_MODEL},
        task_service::TASKS,
    },
};

pub fn notifications_page(cx: Scope) -> Element {
    let notifications_ref = use_atom_ref(cx, NOTIFICATIONS);
    let tasks_ref = use_atom_ref(cx, TASKS);
    let ui_model_ref = use_atom_ref(cx, UI_MODEL);

    let notification_service = use_coroutine_handle::<NotificationCommand>(cx).unwrap();

    let selected_notification: &UseState<Option<Notification>> = use_state(cx, || None);
    let associated_task: &UseState<Option<Task>> = use_state(cx, || None);

    use_future(cx, (), |()| {
        to_owned![notification_service];
        async move {
            notification_service.send(NotificationCommand::Refresh);
        }
    });

    use_memo(
        cx,
        &(
            ui_model_ref.read().selected_notification_index,
            notifications_ref.read().clone(),
            tasks_ref.read().clone(),
        ),
        |(selected_notification_index, notifications, tasks)| {
            if let Some(notification) = notifications.get(selected_notification_index) {
                selected_notification.set(Some(notification.clone()));
                associated_task.set(
                    notification
                        .task_id
                        .and_then(|task_id| tasks.get(&task_id).cloned()),
                );
            }
        },
    );

    cx.render(rsx!(
        div {
            class: "w-full flex-1 overflow-auto",

            div {
                class: "container mx-auto",

                self::notifications_list {
                    notifications: notifications_ref.read().clone(),
                    ui_model_ref: ui_model_ref.clone(),
                    on_delete: |notification: &Notification| {
                        notification_service.send(NotificationCommand::DeleteFromNotification(notification.clone()));
                    },
                    on_unsubscribe: |notification: &Notification| {
                        notification_service.send(NotificationCommand::Unsubscribe(notification.id))
                    },
                    on_snooze: |notification: &Notification| {
                        notification_service.send(NotificationCommand::Snooze(notification.id))
                    },
                    on_complete_task: |notification: &Notification| {
                        notification_service.send(NotificationCommand::CompleteTaskFromNotification(notification.clone()));
                    }
                    on_plan: |notification: &Notification| {
                        if let Some(task_id) = notification.task_id {
                            if let Some(task) = tasks_ref.read().get(&task_id) {
                                ui_model_ref.write().task_planning_modal_opened = true;
                                associated_task.set(Some(task.clone()));
                            }
                        }
                    }
                }
            }
        }

        ui_model_ref.read().task_planning_modal_opened.then(|| {
            associated_task.as_ref().map(|associated_task| {
                rsx!{
                    task_planning_modal {
                        task: associated_task.clone(),
                        on_close: |_| { ui_model_ref.write().task_planning_modal_opened = false; },
                        on_submit: |params| {
                            ui_model_ref.write().task_planning_modal_opened = false;
                            notification_service.send(
                                NotificationCommand::PlanTask(
                                    selected_notification.as_ref().unwrap().clone(),
                                    params
                                )
                            );
                        },
                    }
                }
            })
        })
    ))
}
