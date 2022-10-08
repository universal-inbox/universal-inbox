use crate::components::notifications_list::notifications_list;
use crate::services::notification_service::{
    NotificationCommand, NOTIFICATIONS, SELECTED_NOTIFICATION_INDEX,
};
use dioxus::core::to_owned;
use dioxus::prelude::*;

pub fn notifications_page(cx: Scope) -> Element {
    let notifications = use_atom_ref(&cx, NOTIFICATIONS);
    let selected_notification_index = use_atom_ref(&cx, SELECTED_NOTIFICATION_INDEX);
    let notification_service = use_coroutine_handle::<NotificationCommand>(&cx).unwrap();

    use_future(&cx, (), |()| {
        to_owned![notification_service];
        async move {
            notification_service.send(NotificationCommand::Refresh);
        }
    });
    cx.render(rsx!(
        div {
            class: "container mx-auto px-4",

            self::notifications_list {
                notifications: notifications.read().clone(),
                selected_notification_index: selected_notification_index
            }
        }
    ))
}
