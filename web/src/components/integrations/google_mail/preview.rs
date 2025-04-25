#![allow(non_snake_case)]

use std::collections::HashSet;

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsArrowUpRightSquare, Icon};

use universal_inbox::{
    notification::NotificationWithTask,
    third_party::integrations::google_mail::{
        GoogleMailMessage, GoogleMailThread, GOOGLE_MAIL_IMPORTANT_LABEL, GOOGLE_MAIL_STARRED_LABEL,
    },
    HasHtmlUrl,
};

use crate::components::{integrations::google_mail::icons::Mail, CardWithHeaders, Tag, TagsInCard};

#[component]
pub fn GoogleMailThreadPreview(
    notification: ReadOnlySignal<NotificationWithTask>,
    google_mail_thread: ReadOnlySignal<GoogleMailThread>,
) -> Element {
    let link = notification().get_html_url();
    let labels = google_mail_thread()
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
    let is_starred = google_mail_thread().is_tagged_with(GOOGLE_MAIL_STARRED_LABEL, None);
    let is_important = google_mail_thread().is_tagged_with(GOOGLE_MAIL_IMPORTANT_LABEL, None);
    let mail_icon_style = match (is_starred, is_important) {
        (_, true) => "text-red-500",
        (true, false) => "text-yellow-500",
        _ => "",
    };

    rsx! {
        div {
            class: "flex flex-col gap-2 w-full",

            h3 {
                class: "flex items-center gap-2 text-base",

                Mail { class: "flex-none h-5 w-5 {mail_icon_style}" }
                a {
                    class: "flex items-center",
                    href: "{link}",
                    target: "_blank",
                    "{notification().title}"
                    Icon { class: "h-5 w-5 min-w-5 text-base-content/50 p-1", icon: BsArrowUpRightSquare }
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

            for message in google_mail_thread().messages.into_iter() {
                GoogleMailThreadMessage { message: message }
            }
        }
    }
}

#[component]
fn GoogleMailThreadMessage(message: ReadOnlySignal<GoogleMailMessage>) -> Element {
    let mut headers = vec![];
    if let Some(from) = message().get_header("From") {
        headers
            .push(rsx! { span { class: "text-neutral-content/75", "From:" }, span { "{from}" } });
    }
    if let Some(to) = message().get_header("To") {
        headers.push(rsx! { span { class: "text-neutral-content/75", "To:" }, span { "{to}" } });
    }
    if let Some(cc) = message().get_header("Cc") {
        headers.push(rsx! { span { class: "text-neutral-content/75", "Cc:" }, span { "{cc}" } });
    }
    if let Some(date) = message().get_header("Date") {
        headers
            .push(rsx! { span { class: "text-neutral-content/75", "Date:" }, span { "{date}" } });
    }
    let message_body = use_memo(move || ammonia::clean(&message().render_content_as_html()));

    rsx! {
        CardWithHeaders {
            card_class: "bg-neutral text-neutral-content text-xs",
            headers,

            span {
                class: "prose prose-sm prose-table:text-sm prose-img:max-w-none",
                dangerous_inner_html: "{message_body()}"
            }
        }
    }
}
