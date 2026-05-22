#![allow(non_snake_case)]

use std::collections::HashSet;

use dioxus::prelude::*;

use universal_inbox::{
    notification::NotificationWithTask,
    third_party::integrations::google_mail::{
        GOOGLE_MAIL_IMPORTANT_LABEL, GOOGLE_MAIL_STARRED_LABEL, GoogleMailMessage, GoogleMailThread,
    },
};

use crate::components::{
    Tag, TagDisplay,
    preview_card_header::PreviewCardHeader,
    thread::{Thread, ThreadDivider, ThreadItem},
    threaded_message::ThreadedMessage,
};

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

    // Aggregate labels across every message in the thread, then map each into a
    // `Tag` chip rendered in the preview pane header — same `TagDisplay`
    // component the Linear / Todoist / TickTick previews use for their labels.
    let label_set = google_mail_thread()
        .messages
        .iter()
        .fold(HashSet::new(), |mut acc, msg| {
            if let Some(label_ids) = &msg.label_ids {
                for label in label_ids {
                    acc.insert(label.clone());
                }
            }
            acc
        });
    let mut header_tags: Vec<Tag> = label_set.iter().map(|l| label_to_tag(l)).collect();
    header_tags.sort_by_key(|t| t.get_name());

    // Subject from the first message — render once at the top of the thread.
    let subject = google_mail_thread()
        .messages
        .first()
        .and_then(|m| m.get_header("Subject"))
        .unwrap_or_else(|| notification().title.clone());

    let messages = google_mail_thread().messages.clone();
    let message_count = messages.len();

    rsx! {
        div {
            class: "flex flex-col w-full h-full",

            PreviewCardHeader {
                brand_icon: rsx! { span { class: "icon-[lucide--mail] size-4" } },
                title: subject,
                subline: rsx! {
                    span { "{message_count} message" if message_count != 1 { "s" } }
                    if !header_tags.is_empty() {
                        span { class: "sep", "·" }
                        for tag in header_tags {
                            TagDisplay { tag }
                        }
                    }
                }
            }

            div {
                id: "notification-preview-details",
                class: "flex flex-col gap-2 w-full h-full overflow-y-auto scroll-y-auto p-3",

                div {
                    class: "bg-ui-surface border border-ui-border rounded-ui-lg p-3 mb-2.5",

                    Thread {
                        if let Some(invisible_read_message) = invisible_read_message {
                            ThreadDivider {
                                a {
                                    class: "link link-hover",
                                    style: "cursor: pointer;",
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
    }
}

#[component]
fn GoogleMailThreadMessage(message: ReadSignal<GoogleMailMessage>) -> Element {
    let from_header = message().get_header("From");
    let (sender_name, sender_email) = parse_from_header(from_header.as_deref());
    let to_header = message().get_header("To");
    let cc_header = message().get_header("Cc");
    let internal_date = message().internal_date;

    let message_parts = use_memo(move || {
        let (visible_raw, quoted_raw) = split_quoted_content(&message().render_content_as_html());
        (
            sanitize_email_html(&visible_raw),
            quoted_raw.as_deref().map(sanitize_email_html),
        )
    });
    let mut show_quoted = use_signal(|| false);
    let (visible, quoted) = message_parts();

    let display_name = sender_name.clone().unwrap_or_else(|| {
        sender_email
            .clone()
            .unwrap_or_else(|| "Unknown".to_string())
    });
    let subtitle = sender_email.map(|e| format!("<{e}>"));

    let to_display = to_header.as_deref().map(format_recipient_list);
    let cc_display = cc_header.as_deref().map(format_recipient_list);
    let metadata = if to_display.is_some() || cc_display.is_some() {
        Some(rsx! {
            if let Some(to) = to_display {
                span { strong { "to " } "{to}" }
            }
            if let Some(cc) = cc_display {
                span { strong { "cc " } "{cc}" }
            }
        })
    } else {
        None
    };

    rsx! {
        ThreadItem {
            ThreadedMessage {
                author_name: display_name,
                author_subtitle: subtitle,
                sent_at: Some(internal_date),
                metadata,
                body: rsx! {
                    EmailBodyFrame { html: visible.clone() }
                    if let Some(quoted) = quoted {
                        ThreadDivider {
                            a {
                                class: "link link-hover",
                                style: "cursor: pointer;",
                                onclick: move |_| { *show_quoted.write() = !show_quoted(); },
                                if show_quoted() { "Hide trimmed content" } else { "Show trimmed content" }
                            }
                        }
                        if show_quoted() {
                            EmailBodyFrame { html: quoted.clone() }
                        }
                    }
                },
            }
        }
    }
}

/// Map a Gmail label id to a `Tag` chip, applying semantic styling for the
/// well-known `IMPORTANT` and `STARRED` labels and falling back to the default
/// chip for everything else (system labels like `INBOX` / `UNREAD` / category
/// labels, plus user-defined custom labels).
fn label_to_tag(label: &str) -> Tag {
    match label {
        GOOGLE_MAIL_IMPORTANT_LABEL => Tag::Stylized {
            name: label.to_string(),
            class: "tag warning".to_string(),
        },
        GOOGLE_MAIL_STARRED_LABEL => Tag::Stylized {
            name: label.to_string(),
            class: "tag info".to_string(),
        },
        _ => Tag::Default {
            name: label.to_string(),
        },
    }
}

/// Parse a Gmail "From" header — typically `"Name" <email@host>` or just `email@host` —
/// into separate `(name, email)` parts for the per-message header row.
fn parse_from_header(raw: Option<&str>) -> (Option<String>, Option<String>) {
    let Some(raw) = raw else {
        return (None, None);
    };
    if let (Some(open), Some(close)) = (raw.rfind('<'), raw.rfind('>'))
        && open < close
    {
        let email = raw[open + 1..close].trim().to_string();
        let name = raw[..open].trim().trim_matches('"').to_string();
        let name = if name.is_empty() { None } else { Some(name) };
        let email = if email.is_empty() { None } else { Some(email) };
        return (name, email);
    }
    (Some(raw.trim().to_string()), None)
}

/// Format a comma-separated `To` / `Cc` header value for display: trim each
/// entry, strip surrounding quotes from the display-name part, and rejoin with
/// `", "`. Doesn't try to fully parse RFC 5322 — it just cleans up the common
/// `"Name" <email>, ...` shape that Gmail returns.
fn format_recipient_list(raw: &str) -> String {
    raw.split(',')
        .map(|entry| {
            let entry = entry.trim();
            if let (Some(open), Some(close)) = (entry.rfind('<'), entry.rfind('>'))
                && open < close
            {
                let name = entry[..open].trim().trim_matches('"').trim();
                let email = entry[open + 1..close].trim();
                if name.is_empty() {
                    email.to_string()
                } else {
                    format!("{name} <{email}>")
                }
            } else {
                entry.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
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

/// Permissive sanitization tuned for HTML email content. Layout, inline `style`
/// attributes, and `<style>` blocks are preserved because emails depend on
/// them. Safety comes from the iframe sandbox (no `allow-scripts`), so even if
/// a `<script>` slipped through it would be inert. Ammonia still scrubs JS
/// URLs and event-handler attributes for defense in depth.
fn sanitize_email_html(html: &str) -> String {
    ammonia::Builder::default()
        .rm_clean_content_tags(&["style"])
        .add_tags(&["style", "html", "head", "body", "title", "font", "center"])
        .add_generic_attributes(&[
            "style",
            "class",
            "id",
            "align",
            "valign",
            "width",
            "height",
            "bgcolor",
            "color",
            "border",
            "cellpadding",
            "cellspacing",
        ])
        .add_tag_attributes("img", &["width", "height", "border", "hspace", "vspace"])
        .add_tag_attributes(
            "table",
            &[
                "bgcolor",
                "background",
                "width",
                "height",
                "border",
                "cellpadding",
                "cellspacing",
                "align",
                "valign",
            ],
        )
        .add_tag_attributes(
            "td",
            &[
                "bgcolor",
                "background",
                "width",
                "height",
                "colspan",
                "rowspan",
                "align",
                "valign",
                "nowrap",
            ],
        )
        .add_tag_attributes(
            "th",
            &[
                "bgcolor",
                "background",
                "width",
                "height",
                "colspan",
                "rowspan",
                "align",
                "valign",
                "nowrap",
            ],
        )
        .add_tag_attributes("a", &["target"])
        .url_relative(ammonia::UrlRelative::PassThrough)
        .clean(html)
        .to_string()
}

/// Placeholder host for a sanitized email body. The actual `<iframe srcdoc>`
/// is created from JS (see `web/js/index.js`) once this element is mounted —
/// keeping the heavy srcdoc string off the Dioxus VDOM. On long Gmail threads
/// (20+ messages) embedding the full srcdoc as a Dioxus attribute raced with
/// Dioxus' Callback machinery and panicked with `Dropped(ValueDroppedError)`.
#[component]
fn EmailBodyFrame(html: String) -> Element {
    rsx! {
        div {
            class: "ui-email-frame-host",
            "data-html": "{html}",
        }
    }
}

#[cfg(test)]
mod google_mail_preview_tests {
    use super::{format_recipient_list, split_quoted_content};
    use pretty_assertions::assert_eq;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn format_recipient_list_strips_quotes_and_keeps_email() {
        assert_eq!(
            format_recipient_list(r#""Jane Smith" <jane@example.com>"#),
            "Jane Smith <jane@example.com>"
        );
    }

    #[wasm_bindgen_test]
    fn format_recipient_list_handles_bare_email() {
        assert_eq!(
            format_recipient_list("alice@example.com"),
            "alice@example.com"
        );
    }

    #[wasm_bindgen_test]
    fn format_recipient_list_joins_multiple_recipients() {
        assert_eq!(
            format_recipient_list(
                r#""Jane Smith" <jane@example.com>, bob@example.com, "Carol" <carol@example.com>"#
            ),
            "Jane Smith <jane@example.com>, bob@example.com, Carol <carol@example.com>"
        );
    }

    #[wasm_bindgen_test]
    fn format_recipient_list_drops_empty_name_part() {
        assert_eq!(
            format_recipient_list("<dave@example.com>"),
            "dave@example.com"
        );
    }

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
