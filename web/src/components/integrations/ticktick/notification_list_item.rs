#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{
    notification::{NotificationStatus, NotificationWithTask},
    third_party::integrations::ticktick::{TickTickItem, TickTickTaskStatus},
};

use crate::{
    components::{
        integrations::{
            icons::TickTick,
            ticktick::{list_item::TickTickListItemSubtitle, preview::ticktick_priority_level},
        },
        list::ListItem,
        priority_field::priority_color_class,
    },
    utils::format_elapsed_time,
};

#[component]
pub fn TickTickNotificationListItem(
    notification: ReadSignal<NotificationWithTask>,
    ticktick_item: ReadSignal<TickTickItem>,
    is_selected: ReadSignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let notification_updated_at = use_memo(move || format_elapsed_time(notification().updated_at));
    let is_unread = notification().status == NotificationStatus::Unread;
    let priority = ticktick_item().priority;
    let meta_icon_color_class = ticktick_priority_level(priority)
        .map(priority_color_class)
        .unwrap_or("");
    let meta_icon_class = if ticktick_item().status == TickTickTaskStatus::Completed {
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
                TickTickListItemSubtitle { ticktick_item }
            },
            time: "{notification_updated_at}",
            icon: rsx! { TickTick { class: "h-5 w-5" } },
            meta_icon: rsx! {
                span { class: "{meta_icon_class} {meta_icon_color_class}" }
            },
            is_selected,
            is_unread,
            on_select,
        }
    }
}
