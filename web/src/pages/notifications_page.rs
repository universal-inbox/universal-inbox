use dioxus::core::to_owned;
use dioxus::prelude::*;

use universal_inbox::notification::Notification;

use crate::components::notifications_list::notifications_list;
use crate::services::notification_service::{NotificationCommand, NOTIFICATIONS, UI_MODEL};

pub fn notifications_page(cx: Scope) -> Element {
    let notifications_ref = use_atom_ref(&cx, NOTIFICATIONS);
    let ui_model_ref = use_atom_ref(&cx, UI_MODEL);
    let notification_service = use_coroutine_handle::<NotificationCommand>(&cx).unwrap();

    use_future(&cx, (), |()| {
        to_owned![notification_service];
        async move {
            notification_service.send(NotificationCommand::Refresh);
        }
    });
    cx.render(rsx!(
        div {
            class: "w-full flex-1 overflow-auto",

            div {
                class: "container mx-auto",

                self::notifications_list {
                    notifications: notifications_ref.read().clone(),
                    ui_model_ref: ui_model_ref,
                    on_delete: |notification: &Notification| {
                        notification_service.send(NotificationCommand::DeleteFromNotification(notification.clone()));
                    },
                    on_unsubscribe: |notification: &Notification| {
                        notification_service.send(NotificationCommand::Unsubscribe(notification.id))
                    },
                    on_snooze: |notification: &Notification| {
                        notification_service.send(NotificationCommand::Snooze(notification.id))
                    },
                    on_mark_as_done: |notification: &Notification| {
                        notification_service.send(NotificationCommand::MarkTaskAsDoneFromNotification(notification.clone()));
                    }
                }
            }
        }
    ))
}
