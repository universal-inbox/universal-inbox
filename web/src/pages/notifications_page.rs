use crate::components::notifications_list::notifications_list;
use crate::services::notification_service::{NotificationCommand, NOTIFICATIONS, UI_MODEL};
use dioxus::core::to_owned;
use dioxus::prelude::*;
use universal_inbox::Notification;

pub fn notifications_page(cx: Scope) -> Element {
    let notifications = use_atom_ref(&cx, NOTIFICATIONS);
    let ui_model = use_atom_ref(&cx, UI_MODEL);
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
                    notifications: notifications.read().clone(),
                    ui_model: ui_model,
                    on_mark_as_done: |notification: &Notification| notification_service.send(NotificationCommand::MarkAsDone(notification.clone())),
                }
            }
        }
    ))
}
