#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{Icon, icons::bs_icons::BsLink45deg};

use universal_inbox::{
    HasHtmlUrl, notification::NotificationWithTask, third_party::integrations::api::WebPage,
};

use crate::{
    components::{
        list::{ListContext, ListItem},
        notifications_list::get_notification_list_item_action_buttons,
    },
    icons::UniversalInbox,
    utils::format_elapsed_time,
};

#[component]
pub fn WebPageNotificationListItem(
    notification: ReadSignal<NotificationWithTask>,
    web_page: ReadSignal<WebPage>,
    is_selected: ReadSignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let notification_updated_at = use_memo(move || format_elapsed_time(notification().updated_at));
    let list_context = use_context::<Memo<ListContext>>();
    let link = notification().get_html_url();
    let subicon = if web_page().favicon.is_some() {
        rsx! {
            img {
                class: "h-5 w-5 min-w-5",
                src: "{web_page().favicon.as_ref().unwrap()}",
                alt: "Favicon"
            }
        }
    } else {
        rsx! { Icon { class: "h-5 w-5 min-w-5", icon: BsLink45deg } }
    };

    rsx! {
        ListItem {
            key: "{notification().id}",
            title: "{notification().title}",
            subtitle: rsx! { WebPageListItemSubtitle { web_page } },
            link,
            icon: rsx! { UniversalInbox { class: "h-5 w-5" } },
            subicon,
            action_buttons: get_notification_list_item_action_buttons(
                notification,
                list_context().show_shortcut,
                None,
                None
            ),
            is_selected,
            on_select,

            span { class: "text-base-content/50 whitespace-nowrap text-xs font-mono", "{notification_updated_at}" }
        }
    }
}

#[component]
pub fn WebPageListItemSubtitle(web_page: ReadSignal<WebPage>) -> Element {
    rsx! {
        div {
            class: "flex items-center text-xs text-base-content/50 gap-1",
            span { "{web_page().url}" }
        }
    }
}
