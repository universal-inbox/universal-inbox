#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::notification::{NotificationStatus, NotificationWithTask};

use crate::{
    components::{integrations::icons::Notion, list::ListItem},
    utils::format_elapsed_time,
};

#[component]
pub fn NotionCommentListItem(
    notification: ReadSignal<NotificationWithTask>,
    page_title: ReadSignal<String>,
    is_selected: ReadSignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    rsx! {
        NotionListItem {
            notification,
            page_title,
            meta_icon_class: "icon-[lucide--message-square]",
            is_selected,
            on_select,
        }
    }
}

#[component]
pub fn NotionBlockListItem(
    notification: ReadSignal<NotificationWithTask>,
    page_title: ReadSignal<String>,
    is_selected: ReadSignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    rsx! {
        NotionListItem {
            notification,
            page_title,
            meta_icon_class: "icon-[lucide--at-sign]",
            is_selected,
            on_select,
        }
    }
}

#[component]
fn NotionListItem(
    notification: ReadSignal<NotificationWithTask>,
    page_title: ReadSignal<String>,
    meta_icon_class: &'static str,
    is_selected: ReadSignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let notification_updated_at = use_memo(move || format_elapsed_time(notification().updated_at));
    let is_unread = notification().status == NotificationStatus::Unread;

    rsx! {
        ListItem {
            key: "{notification().id}",
            linked_task: notification().task,
            title: "{notification().title}",
            subtitle: rsx! {
                span { class: "ui-nrow-meta-text", "{page_title}" }
            },
            time: "{notification_updated_at}",
            icon: rsx! { Notion { class: "h-5 w-5" } },
            meta_icon: rsx! { span { class: "{meta_icon_class} w-full h-full" } },
            is_selected,
            is_unread,
            on_select,
        }
    }
}
