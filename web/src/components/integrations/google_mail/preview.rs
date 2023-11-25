#![allow(non_snake_case)]

use std::collections::HashSet;

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsArrowUpRightSquare, Icon};

use universal_inbox::{
    notification::{
        integrations::google_mail::{
            GoogleMailMessage, GoogleMailThread, GOOGLE_MAIL_IMPORTANT_LABEL,
            GOOGLE_MAIL_STARRED_LABEL,
        },
        NotificationWithTask,
    },
    HasHtmlUrl,
};

use crate::components::{icons::Mail, SmallCard, Tag, TagsInCard};

#[inline_props]
pub fn GoogleMailThreadPreview<'a>(
    cx: Scope,
    notification: &'a NotificationWithTask,
    google_mail_thread: GoogleMailThread,
) -> Element {
    let link = notification.get_html_url();
    let labels = google_mail_thread
        .messages
        .iter()
        .fold(HashSet::new(), |mut acc, msg| {
            if let Some(labels) = &msg.label_ids {
                for label in labels {
                    acc.insert(label.clone());
                }
            }
            acc
        });
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

            TagsInCard {
                tags: labels
                    .iter()
                    .map(|label| {
                        let class = match label.as_str() {
                            GOOGLE_MAIL_IMPORTANT_LABEL => Some("bg-red-500".to_string()),
                            GOOGLE_MAIL_STARRED_LABEL => Some("bg-yellow-500".to_string()),
                            _ => None,
                        };
                        if let Some(class) = class {
                            Tag::Stylized { name: label.clone(), class }
                        } else {
                            Tag::Default { name: label.clone() }
                        }
                    })
                    .collect()
            }

            for message in google_mail_thread.messages.iter() {
                render! { GoogleMailThreadMessage { message: message } }
            }
        }
    }
}

#[inline_props]
fn GoogleMailThreadMessage<'a>(cx: Scope, message: &'a GoogleMailMessage) -> Element {
    let from = message.get_header("From");
    let to = message.get_header("To");
    let date = message.get_header("Date");

    render! {
        div {
            class: "card w-full bg-base-200 text-base-content",
            div {
                class: "card-body flex flex-col gap-2 p-2",

                if let Some(from) = from {
                    render! {
                        SmallCard {
                            class: "text-xs",
                            card_class: "bg-neutral text-neutral-content",
                            span { class: "text-gray-400", "From:" }
                            span { "{from}" }
                        }
                    }
                }

                if let Some(to) = to {
                    render! {
                        SmallCard {
                            class: "text-xs",
                            card_class: "bg-neutral text-neutral-content",
                            span { class: "text-gray-400", "To:" }
                            span { "{to}" }
                        }
                    }
                }

                if let Some(date) = date {
                    render! {
                        SmallCard {
                            class: "text-xs",
                            card_class: "bg-neutral text-neutral-content",
                            span { class: "text-gray-400", "Date:" }
                            span { "{date}" }
                        }
                    }
                }

                span { dangerous_inner_html: "{message.snippet} &hellip;" }
            }
        }
    }
}
