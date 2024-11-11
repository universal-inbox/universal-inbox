#![allow(non_snake_case)]

use std::collections::HashSet;

use chrono::{DateTime, Local};
use dioxus::prelude::*;

use dioxus_free_icons::{
    icons::bs_icons::{BsExclamationCircle, BsStar},
    Icon,
};
use universal_inbox::{
    notification::NotificationWithTask,
    third_party::integrations::google_mail::{
        GoogleMailThread, MessageSelection, GOOGLE_MAIL_IMPORTANT_LABEL, GOOGLE_MAIL_STARRED_LABEL,
    },
};

use crate::components::{
    integrations::google_mail::icons::{GoogleMail, Mail},
    list::{ListContext, ListItem},
    notifications_list::{get_notification_list_item_action_buttons, TaskHint},
};

#[component]
pub fn GoogleMailThreadListItem(
    notification: ReadOnlySignal<NotificationWithTask>,
    google_mail_thread: ReadOnlySignal<GoogleMailThread>,
    is_selected: ReadOnlySignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let notification_updated_at = use_memo(move || {
        Into::<DateTime<Local>>::into(notification().updated_at)
            .format("%Y-%m-%d %H:%M")
            .to_string()
    });
    let list_context = use_context::<Memo<ListContext>>();
    let is_starred = google_mail_thread().is_tagged_with(GOOGLE_MAIL_STARRED_LABEL, None);
    let is_important = google_mail_thread().is_tagged_with(GOOGLE_MAIL_IMPORTANT_LABEL, None);
    let mail_icon_style = match (is_starred, is_important) {
        (_, true) => "text-red-500",
        (true, false) => "text-yellow-500",
        _ => "",
    };

    rsx! {
        ListItem {
            key: "{notification().id}",
            title: "{notification().title}",
            subtitle: rsx! { GoogleMailThreadSubtitle { google_mail_thread } },
            icon: rsx! { GoogleMail { class: "h-5 w-5" }, TaskHint { task: notification().task } },
            subicon: rsx! { Mail { class: "h-5 w-5 min-w-5 {mail_icon_style}" } },
            action_buttons: get_notification_list_item_action_buttons(
                notification,
                list_context().show_shortcut
            ),
            is_selected,
            on_select,

            if is_starred {
                Icon { class: "mx-0.5 h-5 w-5 text-yellow-500", icon: BsStar }
            }
            if is_important {
                Icon { class: "mx-0.5 h-5 w-5 text-red-500", icon: BsExclamationCircle }
            }

            span { class: "text-gray-400 whitespace-nowrap text-xs font-mono", "{notification_updated_at}" }
        }
    }
}

#[component]
fn GoogleMailThreadSubtitle(google_mail_thread: ReadOnlySignal<GoogleMailThread>) -> Element {
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
            class: "flex gap-2 text-xs text-gray-400",

            if let Some(from_address) = from_address {
                span { "{from_address}" }
            }
            span { "({interlocutors_count})" }
        }
    }
}
