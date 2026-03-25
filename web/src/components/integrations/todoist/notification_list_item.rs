#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{
    notification::{NotificationStatus, NotificationWithTask},
    third_party::integrations::todoist::TodoistItem,
};

use crate::{
    components::{
        integrations::todoist::{
            icons::Todoist, list_item::TodoistListItemSubtitle, preview::todoist_priority_level,
        },
        list::ListItem,
        priority_field::priority_color_class,
    },
    utils::format_elapsed_time,
};

#[component]
pub fn TodoistNotificationListItem(
    notification: ReadSignal<NotificationWithTask>,
    todoist_item: ReadSignal<TodoistItem>,
    is_selected: ReadSignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let notification_updated_at = use_memo(move || format_elapsed_time(notification().updated_at));
    let is_unread = notification().status == NotificationStatus::Unread;
    let priority = todoist_item().priority;
    let meta_icon_color_class = priority_color_class(todoist_priority_level(priority));
    let meta_icon_class = if todoist_item().checked {
        "icon-[lucide--check-circle] w-full h-full"
    } else {
        "icon-[lucide--circle] w-full h-full"
    };

    rsx! {
        ListItem {
            key: "{notification().id}",
            linked_task: notification().task,
            title: "{notification().title}",
            subtitle: rsx! {
                TodoistListItemSubtitle { todoist_item }
            },
            time: "{notification_updated_at}",
            icon: rsx! {
                Todoist { class: "h-5 w-5" },
            },
            meta_icon: rsx! {
                span { class: "{meta_icon_class} {meta_icon_color_class}" }
            },
            is_selected,
            is_unread,
            on_select,
        }
    }
}
