#![allow(non_snake_case)]

use std::collections::HashSet;

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsArrowUpRightSquare, Icon};

use universal_inbox::{
    notification::{
        integrations::google_mail::{
            GoogleMailThread, MessageSelection, GOOGLE_MAIL_IMPORTANT_LABEL,
            GOOGLE_MAIL_STARRED_LABEL,
        },
        NotificationWithTask,
    },
    HasHtmlUrl,
};

use crate::components::icons::Mail;

#[inline_props]
pub fn GoogleMailThreadPreview<'a>(
    cx: Scope,
    notification: &'a NotificationWithTask,
    google_mail_thread: GoogleMailThread,
) -> Element {
    let link = notification.get_html_url();
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
    let is_starred = google_mail_thread.is_tagged_with(GOOGLE_MAIL_STARRED_LABEL, None);
    let is_important = google_mail_thread.is_tagged_with(GOOGLE_MAIL_IMPORTANT_LABEL, None);
    let mail_icon_style = match (is_starred, is_important) {
        (_, true) => "text-red-500",
        (true, false) => "text-yellow-500",
        _ => "",
    };

    render! {
        div {
            class: "flex flex-col gap-2 w-full",

            div {
                class: "flex gap-2",

                if let Some(from_address) = from_address {
                    render! {
                        span { class: "text-xs text-gray-400", "From: {from_address}" }
                        span { class: "text-xs text-gray-400", "({interlocutors_count})" }
                    }
                }
            }

            h2 {
                class: "flex items-center gap-2 text-lg",

                Mail { class: "flex-none h-5 w-5 {mail_icon_style}" }
                a {
                    href: "{link}",
                    target: "_blank",
                    "{notification.title}"
                }
                a {
                    class: "flex-none",
                    href: "{link}",
                    target: "_blank",
                    Icon { class: "h-5 w-5 text-gray-400 p-1", icon: BsArrowUpRightSquare }
                }
            }
        }
    }
}
