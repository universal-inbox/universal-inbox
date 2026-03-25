#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{
    notification::{NotificationStatus, NotificationWithTask},
    third_party::integrations::api::WebPage,
};

use crate::{components::list::ListItem, icons::UILogo, utils::format_elapsed_time};

#[component]
pub fn WebPageNotificationListItem(
    notification: ReadSignal<NotificationWithTask>,
    web_page: ReadSignal<WebPage>,
    is_selected: ReadSignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let notification_updated_at = use_memo(move || format_elapsed_time(notification().updated_at));
    let is_unread = notification().status == NotificationStatus::Unread;
    let meta_icon = if let Some(favicon) = web_page().favicon.as_ref() {
        rsx! {
            img {
                class: "w-full h-full",
                src: "{favicon}",
                alt: ""
            }
        }
    } else {
        rsx! { span { class: "icon-[lucide--globe] w-full h-full" } }
    };

    rsx! {
        ListItem {
            key: "{notification().id}",
            linked_task: notification().task,
            title: "{notification().title}",
            subtitle: rsx! { WebPageListItemSubtitle { web_page } },
            time: "{notification_updated_at}",
            icon: rsx! { UILogo { class: "h-5 w-5".to_string() } },
            meta_icon,
            is_selected,
            is_unread,
            on_select,
        }
    }
}

#[component]
pub fn WebPageListItemSubtitle(web_page: ReadSignal<WebPage>) -> Element {
    rsx! {
        span {
            class: "ui-nrow-meta-text",
            "{web_page().url}"
        }
    }
}
