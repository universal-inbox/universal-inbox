#![allow(non_snake_case)]

use std::collections::HashSet;

use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::bs_icons::{BsExclamationCircle, BsStar},
};
use universal_inbox::{
    HasHtmlUrl,
    notification::NotificationWithTask,
    third_party::integrations::google_mail::{
        GOOGLE_MAIL_IMPORTANT_LABEL, GOOGLE_MAIL_STARRED_LABEL, GoogleMailThread, MessageSelection,
    },
};

use crate::{
    components::{
        integrations::google_mail::icons::{GoogleMail, Mail},
        list::{ListContext, ListItem},
        notifications_list::{TaskHint, get_notification_list_item_action_buttons},
    },
    utils::format_elapsed_time,
};

#[component]
pub fn GoogleMailThreadListItem(
    notification: ReadSignal<NotificationWithTask>,
    google_mail_thread: ReadSignal<GoogleMailThread>,
    is_selected: ReadSignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let notification_updated_at = use_memo(move || format_elapsed_time(notification().updated_at));
    let list_context = use_context::<Memo<ListContext>>();
    let is_starred = google_mail_thread().is_tagged_with(GOOGLE_MAIL_STARRED_LABEL, None);
    let is_important = google_mail_thread().is_tagged_with(GOOGLE_MAIL_IMPORTANT_LABEL, None);
    let mail_icon_style = match (is_starred, is_important) {
        (_, true) => "text-red-500",
        (true, false) => "text-yellow-500",
        _ => "",
    };
    let link = notification().get_html_url();

    rsx! {
        ListItem {
            key: "{notification().id}",
            title: "{notification().title}",
            link,
            subtitle: rsx! { GoogleMailThreadSubtitle { google_mail_thread } },
            icon: rsx! {
                GoogleMail { class: "h-5 w-5" },
                TaskHint { task: notification().task }
            },
            subicon: rsx! { Mail { class: "h-5 w-5 min-w-5 {mail_icon_style}" } },
            action_buttons: get_notification_list_item_action_buttons(
                notification,
                list_context().show_shortcut,
                None,
                None
            ),
            is_selected,
            on_select,

            if is_starred {
                Icon { class: "mx-0.5 h-5 w-5 text-yellow-500", icon: BsStar }
            }
            if is_important {
                Icon { class: "mx-0.5 h-5 w-5 text-red-500", icon: BsExclamationCircle }
            }

            span { class: "text-base-content/50 whitespace-nowrap text-xs font-mono", "{notification_updated_at}" }
        }
    }
}

#[component]
fn GoogleMailThreadSubtitle(google_mail_thread: ReadSignal<GoogleMailThread>) -> Element {
    let from_address = google_mail_thread().get_message_header(MessageSelection::First, "From");
    let interlocutors_count = google_mail_thread()
        .messages
        .iter()
        .fold(HashSet::new(), |mut acc, msg| {
            if let Some(from_address) = msg.get_header("From") {
                acc.insert(from_address);
            }
            acc
        })
        .len();

    rsx! {
        div {
            class: "flex gap-2 text-xs text-base-content/50",

            if let Some(from_address) = from_address {
                span { "{from_address}" }
            }
            span { "({interlocutors_count})" }
        }
    }
}
