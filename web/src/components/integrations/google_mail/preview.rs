#![allow(non_snake_case)]

use std::collections::HashSet;

use dioxus::prelude::*;
use dioxus_free_icons::{Icon, icons::bs_icons::BsArrowUpRightSquare};

use universal_inbox::{
    HasHtmlUrl,
    notification::NotificationWithTask,
    third_party::integrations::google_mail::{
        GOOGLE_MAIL_IMPORTANT_LABEL, GOOGLE_MAIL_STARRED_LABEL, GoogleMailMessage, GoogleMailThread,
    },
};

use crate::components::{CardWithHeaders, Tag, TagsInCard, integrations::google_mail::icons::Mail};

#[component]
pub fn GoogleMailThreadPreview(
    notification: ReadSignal<NotificationWithTask>,
    google_mail_thread: ReadSignal<GoogleMailThread>,
    expand_details: ReadSignal<bool>,
) -> Element {
    let mut show_all = use_signal(|| false);
    let _resource = use_resource(move || async move {
        *show_all.write() = expand_details();
    });
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
    let invisible_read_message = if show_all() {
        None
    } else {
        let read_messages = google_mail_thread()
            .messages
            .iter()
            .filter(|m| m.is_read())
            .collect::<Vec<_>>()
            .len();
        match read_messages {
            0 => None,
            1 => Some("1 hidden message...".to_string()),
            n => Some(format!("{n} hidden messages...")),
        }
    };
    let mut tags: Vec<_> = labels
        .iter()
        .map(|label| {
            let class = match label.as_str() {
                GOOGLE_MAIL_IMPORTANT_LABEL => Some("bg-red-500".to_string()),
                GOOGLE_MAIL_STARRED_LABEL => Some("bg-yellow-500".to_string()),
                _ => None,
            };
            if let Some(class) = class {
                Tag::Stylized {
                    name: label.clone(),
                    class,
                }
            } else {
                Tag::Default {
                    name: label.clone(),
                }
            }
        })
        .collect();
    tags.sort_by_key(|t| t.get_name());

    rsx! {
        div {
            class: "flex flex-col gap-2 w-full h-full",

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

            div {
                id: "notification-preview-details",
                class: "flex flex-col gap-2 w-full h-full overflow-y-auto scroll-y-auto",

                TagsInCard { tags }

                if let Some(invisible_read_message) = invisible_read_message {
                    div {
                        class: "divider divider-primary text-xs",
                        a {
                            class: "link link-hover link-primary",
                            onclick: move |_| { *show_all.write() = true; },
                            "{invisible_read_message}"
                        }
                    }
                }
                for message in google_mail_thread().messages.into_iter() {
                    if show_all() || !message.is_read() {
                        GoogleMailThreadMessage { message }
                    }
                }
            }
        }
    }
}

#[component]
fn GoogleMailThreadMessage(message: ReadSignal<GoogleMailMessage>) -> Element {
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
    let message_parts = use_memo(move || {
        let sanitized = ammonia::Builder::default()
            .set_tag_attribute_value("a", "target", "_blank")
            .add_tag_attributes("div", &["class"])
            .add_tag_attributes("blockquote", &["class", "type"])
            .clean(&message().render_content_as_html())
            .to_string();
        split_quoted_content(&sanitized)
    });
    let mut show_quoted = use_signal(|| false);
    let (visible, quoted) = message_parts();

    rsx! {
        CardWithHeaders {
            card_class: "bg-neutral text-neutral-content text-xs",
            headers,

            span {
                class: "prose prose-sm prose-table:text-sm prose-img:max-w-none",
                dangerous_inner_html: "{visible}"
            }
            if let Some(quoted) = quoted {
                button {
                    class: "btn btn-xs btn-ghost px-2 py-0 min-h-0 h-5 align-middle text-neutral-content/60 hover:text-neutral-content",
                    title: if show_quoted() { "Hide quoted content" } else { "Show quoted content" },
                    onclick: move |_| { *show_quoted.write() = !show_quoted(); },
                    "…"
                }
                if show_quoted() {
                    span {
                        class: "prose prose-sm prose-table:text-sm prose-img:max-w-none",
                        dangerous_inner_html: "{quoted}"
                    }
                }
            }
        }
    }
}

fn split_quoted_content(html: &str) -> (String, Option<String>) {
    const MARKERS: &[&str] = &[
        "<div class=\"gmail_quote",
        "<blockquote type=\"cite\"",
        "<blockquote class=\"gmail_quote",
    ];
    let earliest = MARKERS.iter().filter_map(|m| html.find(m)).min();
    match earliest {
        Some(pos) => (html[..pos].to_string(), Some(html[pos..].to_string())),
        None => (html.to_string(), None),
    }
}

#[cfg(test)]
mod google_mail_preview_tests {
    use super::split_quoted_content;
    use pretty_assertions::assert_eq;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn no_quote_markers_returns_whole_html_and_none() {
        let html = "<p>hello world</p>";
        let (visible, quoted) = split_quoted_content(html);
        assert_eq!(visible, html);
        assert_eq!(quoted, None);
    }

    #[wasm_bindgen_test]
    fn splits_at_gmail_quote_div() {
        let html = r#"<p>new reply</p><div class="gmail_quote gmail_quote_container"><blockquote>old</blockquote></div>"#;
        let (visible, quoted) = split_quoted_content(html);
        assert_eq!(visible, "<p>new reply</p>");
        assert_eq!(
            quoted.as_deref(),
            Some(
                r#"<div class="gmail_quote gmail_quote_container"><blockquote>old</blockquote></div>"#
            )
        );
    }

    #[wasm_bindgen_test]
    fn splits_at_blockquote_type_cite() {
        let html = r#"<p>new reply</p><blockquote type="cite">old</blockquote>"#;
        let (visible, quoted) = split_quoted_content(html);
        assert_eq!(visible, "<p>new reply</p>");
        assert_eq!(
            quoted.as_deref(),
            Some(r#"<blockquote type="cite">old</blockquote>"#)
        );
    }

    #[wasm_bindgen_test]
    fn splits_at_blockquote_class_gmail_quote() {
        let html = r#"reply<br><blockquote class="gmail_quote">quoted</blockquote>"#;
        let (visible, quoted) = split_quoted_content(html);
        assert_eq!(visible, "reply<br>");
        assert_eq!(
            quoted.as_deref(),
            Some(r#"<blockquote class="gmail_quote">quoted</blockquote>"#)
        );
    }

    #[wasm_bindgen_test]
    fn earliest_marker_wins_when_multiple_present() {
        let html =
            r#"<p>reply</p><blockquote type="cite">A</blockquote><div class="gmail_quote">B</div>"#;
        let (visible, quoted) = split_quoted_content(html);
        assert_eq!(visible, "<p>reply</p>");
        assert!(quoted.unwrap().starts_with(r#"<blockquote type="cite""#));
    }
}
