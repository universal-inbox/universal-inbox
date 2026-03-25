#![allow(non_snake_case)]

use std::collections::HashSet;

use dioxus::prelude::*;
use universal_inbox::{
    notification::{NotificationStatus, NotificationWithTask},
    third_party::integrations::google_mail::{GoogleMailThread, MessageSelection},
};

use crate::{
    components::{integrations::google_mail::icons::GoogleMail, list::ListItem},
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
    let is_unread = notification().status == NotificationStatus::Unread;

    rsx! {
        ListItem {
            key: "{notification().id}",
            linked_task: notification().task,
            title: "{notification().title}",
            subtitle: rsx! {
                GoogleMailThreadSubtitle { google_mail_thread }
            },
            time: "{notification_updated_at}",
            icon: rsx! {
                GoogleMail { class: "h-5 w-5" },
            },
            meta_icon: rsx! { span { class: "icon-[lucide--mail] w-full h-full" } },
            is_selected,
            is_unread,
            on_select,
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
        span {
            class: "ui-nrow-meta-text",
            if let Some(from_address) = from_address {
                "{from_address} ({interlocutors_count})"
            } else {
                "({interlocutors_count})"
            }
        }
    }
}
