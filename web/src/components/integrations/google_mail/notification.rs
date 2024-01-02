#![allow(non_snake_case)]

use std::collections::HashSet;

use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{BsExclamationCircle, BsStar},
    Icon,
};

use universal_inbox::notification::{
    integrations::google_mail::{
        GoogleMailThread, MessageSelection, GOOGLE_MAIL_IMPORTANT_LABEL, GOOGLE_MAIL_STARRED_LABEL,
    },
    NotificationWithTask,
};

use crate::components::icons::Mail;

#[component]
pub fn GoogleMailThreadDisplay<'a>(
    cx: Scope,
    notif: &'a NotificationWithTask,
    google_mail_thread: GoogleMailThread,
) -> Element {
    let is_starred = google_mail_thread.is_tagged_with(GOOGLE_MAIL_STARRED_LABEL, None);
    let is_important = google_mail_thread.is_tagged_with(GOOGLE_MAIL_IMPORTANT_LABEL, None);
    let from_address = google_mail_thread.get_message_header(MessageSelection::First, "From");
    let interlocutors_count = google_mail_thread
        .messages
        .iter()
        .fold(HashSet::new(), |mut acc, msg| {
            if let Some(from_address) = msg.get_header("From") {
                acc.insert(from_address);
            }
            acc
        })
        .len();
    let mail_icon_style = match (is_starred, is_important) {
        (_, true) => "text-red-500",
        (true, false) => "text-yellow-500",
        _ => "",
    };

    render! {
        div {
            class: "flex items-center gap-2",

            Mail { class: "h-5 w-5 {mail_icon_style}" }

            div {
                class: "flex flex-col grow",

                span { class: "mx-0.5", "{notif.title}" }
                div {
                    class: "flex gap-2",

                    if let Some(from_address) = from_address {
                        render! { span { class: "text-xs text-gray-400", "{from_address}" } }
                    }
                    span { class: "text-xs text-gray-400", "({interlocutors_count})" }
                }
            }
        }
    }
}

#[component]
pub fn GoogleMailNotificationDetailsDisplay<'a>(
    cx: Scope,
    google_mail_thread: &'a GoogleMailThread,
) -> Element {
    let is_starred = google_mail_thread.is_tagged_with(GOOGLE_MAIL_STARRED_LABEL, None);
    let is_important = google_mail_thread.is_tagged_with(GOOGLE_MAIL_IMPORTANT_LABEL, None);

    render! {
        div {
            class: "flex gap-2",

            if is_starred {
                render! { Icon { class: "mx-0.5 h-5 w-5 text-yellow-500", icon: BsStar } }
            }
            if is_important {
                render! { Icon { class: "mx-0.5 h-5 w-5 text-red-500", icon: BsExclamationCircle } }
            }
        }
    }
}
