#![allow(non_snake_case)]

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use notion_blocks_render::{NotionRenderOptions, render_rich_text_as_html};
use notionrs_types::object::rich_text::RichText;

use universal_inbox::{
    notification::NotificationWithTask,
    third_party::integrations::notion::{
        NotionBlock, NotionComment, NotionCommentMessage, NotionUser,
    },
};

use crate::{
    components::{
        preview_card_header::PreviewCardHeader,
        thread::{
            EarlierMessagesDivider, LATEST_READ_ANCHOR_ID, NewMessagesDivider,
            ShowThreadStartDivider, use_thread_collapse,
        },
        ui::{Tag, TagVariant, thread_message::ThreadedMessage},
    },
    utils::{format_elapsed_time, sanitize_html_keeping_span_classes},
};

const UNKNOWN_AUTHOR_NAME: &str = "Someone";

#[component]
pub fn NotionCommentPreview(
    notification: ReadSignal<NotificationWithTask>,
    notion_comment: ReadSignal<NotionComment>,
    expand_details: ReadSignal<bool>,
) -> Element {
    let collapse = use_thread_collapse("notification-preview-details", expand_details, move || {
        let comment = notion_comment();
        let (read_replies, _) = split_replies(&comment, notification().last_read_at);
        (comment.discussion_id.clone(), !read_replies.is_empty())
    });
    let mut show_all = collapse.show_all;
    let mut show_root = collapse.show_root;

    let comment = notion_comment();
    let user_notion_id = comment.user_notion_id.clone();
    let root = comment.comments.first().clone();
    let (read_replies, unread_replies) = split_replies(&comment, notification().last_read_at);
    let unread_count = unread_replies.len();

    // With read replies, the root is hidden behind its own control: the latest
    // read reply is the context that matters for triage.
    let root_hidden = !read_replies.is_empty() && !show_root();
    let collapse_active = !show_all() && read_replies.len() >= 2;
    let hidden_count = read_replies.len().saturating_sub(1);
    let mut shown_read_replies = if collapse_active {
        read_replies.last().cloned().into_iter().collect()
    } else {
        read_replies
    };
    let latest_read_reply = shown_read_replies.pop();

    let latest_comment = comment.latest_comment();
    let header_author = author_name(&root.author);
    let header_age = format_elapsed_time(latest_comment.last_edited_time);
    let quoted_html = comment
        .quoted_content
        .as_ref()
        .map(|quoted_content| render_html(quoted_content, user_notion_id.as_deref()));

    rsx! {
        div {
            class: "flex flex-col w-full h-full",

            PreviewCardHeader {
                brand_icon: rsx! { span { class: "icon-[lucide--file-text] size-4" } },
                title: comment.page.title.clone(),
                subline: rsx! {
                    span { class: "icon-[lucide--message-square] size-3" }
                    span { "Comment by" }
                    span {
                        style: "color: var(--ui-base-content); font-weight: 500;",
                        "{header_author}"
                    }
                    span { class: "sep", "·" }
                    span { "{header_age} ago" }
                }
            }

            div {
                id: "notification-preview-details",
                class: "flex flex-col gap-2 w-full h-full overflow-y-auto scroll-y-auto p-3",

                // Commented block: pull-quote with left rule, distinct from the comments.
                if let Some(quoted_html) = quoted_html {
                    div {
                        class: "border-l-[3px] border-solid border-ui-primary-light bg-ui-primary-subtle px-3 py-2 text-[12.5px] italic rounded-r-ui-sm text-ui-base-content",
                        dangerous_inner_html: "{quoted_html}"
                    }
                }

                div {
                    class: "bg-ui-surface border border-ui-border rounded-ui-lg p-3 mb-2.5",

                    if root_hidden {
                        ShowThreadStartDivider {
                            onclick: move |_| { *show_root.write() = true; },
                        }
                    } else {
                        NotionCommentMessageDisplay {
                            message: root.clone(),
                            user_notion_id: user_notion_id.clone(),
                        }
                    }

                    if collapse_active {
                        EarlierMessagesDivider {
                            hidden_count,
                            singular: "reply",
                            plural: "replies",
                            onclick: move |_| { *show_all.write() = true; },
                        }
                    } else {
                        for message in shown_read_replies.iter().cloned() {
                            NotionCommentMessageDisplay {
                                key: "{message.id}",
                                message,
                                user_notion_id: user_notion_id.clone(),
                            }
                        }
                    }

                    if let Some(message) = latest_read_reply {
                        div {
                            id: LATEST_READ_ANCHOR_ID,
                            NotionCommentMessageDisplay {
                                message,
                                user_notion_id: user_notion_id.clone(),
                            }
                        }
                    }

                    if unread_count > 0 {
                        NewMessagesDivider {
                            unread_count,
                            singular: "reply",
                            plural: "replies",
                        }
                        for message in unread_replies.iter().cloned() {
                            NotionCommentMessageDisplay {
                                key: "{message.id}",
                                message,
                                user_notion_id: user_notion_id.clone(),
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn NotionCommentMessageDisplay(
    message: ReadSignal<NotionCommentMessage>,
    user_notion_id: ReadSignal<Option<String>>,
) -> Element {
    let message = message();
    let html = render_html(&message.rich_text, user_notion_id().as_deref());

    rsx! {
        ThreadedMessage {
            author_name: author_name(&message.author),
            author_avatar_url: message.author.avatar_url.clone(),
            sent_at: Some(message.created_time),
            body: rsx! {
                div {
                    class: "prose prose-sm max-w-none",
                    dangerous_inner_html: "{html}"
                }
            },
        }
    }
}

#[component]
pub fn NotionBlockPreview(notion_block: ReadSignal<NotionBlock>) -> Element {
    let block = notion_block();
    let editor_name = block
        .last_edited_by
        .as_ref()
        .map(author_name)
        .unwrap_or_else(|| UNKNOWN_AUTHOR_NAME.to_string());
    let edited_age = format_elapsed_time(block.last_edited_time);
    let block_type = block_type_label(&block.block_type);
    let html = render_html(&block.rich_text, block.user_notion_id.as_deref());

    rsx! {
        div {
            class: "flex flex-col w-full h-full",

            PreviewCardHeader {
                brand_icon: rsx! { span { class: "icon-[lucide--file-text] size-4" } },
                title: block.page.title.clone(),
                subline: rsx! {
                    span { class: "icon-[lucide--at-sign] size-3" }
                    span { "Mentioned by" }
                    span {
                        style: "color: var(--ui-base-content); font-weight: 500;",
                        "{editor_name}"
                    }
                    span { class: "sep", "·" }
                    span { "{edited_age} ago" }
                    if let Some(block_type) = block_type {
                        span { class: "sep", "·" }
                        Tag { variant: TagVariant::Muted, "{block_type}" }
                    }
                }
            }

            div {
                id: "notification-preview-details",
                class: "flex flex-col gap-2 w-full h-full overflow-y-auto scroll-y-auto p-3",

                div {
                    class: "bg-ui-surface border border-ui-border rounded-ui-lg p-3 mb-2.5",

                    ThreadedMessage {
                        author_name: editor_name,
                        author_avatar_url: block.last_edited_by.as_ref().and_then(|user| user.avatar_url.clone()),
                        sent_at: Some(block.last_edited_time),
                        body: rsx! {
                            div {
                                class: "prose prose-sm max-w-none",
                                dangerous_inner_html: "{html}"
                            }
                        },
                    }
                }
            }
        }
    }
}

/// Render Notion rich text as sanitized HTML, highlighting the mentions of
/// the current user.
fn render_html(rich_text: &[RichText], user_notion_id: Option<&str>) -> String {
    sanitize_html_keeping_span_classes(&render_rich_text_as_html(
        rich_text,
        &NotionRenderOptions {
            default_class: "text-primary",
            highlight_class: "text-warning",
            user_id_to_highlight: user_notion_id,
        },
    ))
}

/// Split the replies into those already read and the unread ones. A
/// notification never read has only unread replies.
fn split_replies(
    comment: &NotionComment,
    last_read_at: Option<DateTime<Utc>>,
) -> (Vec<NotionCommentMessage>, Vec<NotionCommentMessage>) {
    match last_read_at {
        Some(last_read_at) => {
            let (read, unread) = comment.split_seen_replies(last_read_at);
            (read.to_vec(), unread.to_vec())
        }
        None => (vec![], comment.comments[1..].to_vec()),
    }
}

fn author_name(user: &NotionUser) -> String {
    user.name
        .as_ref()
        .map(|name| name.expose().trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| UNKNOWN_AUTHOR_NAME.to_string())
}

/// Human label of a Notion block type, `None` for a plain paragraph.
fn block_type_label(block_type: &str) -> Option<String> {
    if block_type == "paragraph" || block_type.is_empty() {
        return None;
    }
    let label = block_type.replace('_', " ");
    let mut chars = label.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use universal_inbox::{
        pii::Pii,
        third_party::integrations::notion::{NotionCommentParent, NotionPage},
    };
    use vec1::vec1;
    use wasm_bindgen_test::*;

    fn notion_user(name: Option<&str>) -> NotionUser {
        NotionUser {
            id: "user-1".to_string(),
            name: name.map(|name| Pii::new(name.to_string())),
            avatar_url: None,
        }
    }

    fn message(id: &str, minute: u32) -> NotionCommentMessage {
        let time = Utc.with_ymd_and_hms(2026, 10, 10, 9, minute, 0).unwrap();
        NotionCommentMessage {
            id: id.to_string(),
            author: notion_user(Some("Jane Doe")),
            rich_text: vec![RichText::from(id)],
            created_time: time,
            last_edited_time: time,
        }
    }

    fn notion_comment() -> NotionComment {
        NotionComment {
            discussion_id: "discussion-1".to_string(),
            page: NotionPage {
                id: "page-1".to_string(),
                title: "Roadmap".to_string(),
                url: "https://www.notion.so/Roadmap-1".parse().unwrap(),
                created_by: None,
            },
            parent: NotionCommentParent::Page {
                page_id: "page-1".to_string(),
            },
            quoted_content: None,
            parent_block_created_by: None,
            comments: vec1![message("c1", 0), message("c2", 10), message("c3", 20)],
            user_notion_id: Some("user-1".to_string()),
        }
    }

    fn ids(messages: &[NotionCommentMessage]) -> Vec<&str> {
        messages.iter().map(|message| message.id.as_str()).collect()
    }

    #[wasm_bindgen_test]
    fn test_split_replies_never_read() {
        let (read, unread) = split_replies(&notion_comment(), None);

        assert!(read.is_empty());
        assert_eq!(ids(&unread), vec!["c2", "c3"]);
    }

    #[wasm_bindgen_test]
    fn test_split_replies_partially_read() {
        let last_read_at = Utc.with_ymd_and_hms(2026, 10, 10, 9, 15, 0).unwrap();

        let (read, unread) = split_replies(&notion_comment(), Some(last_read_at));

        assert_eq!(ids(&read), vec!["c2"]);
        assert_eq!(ids(&unread), vec!["c3"]);
    }

    #[wasm_bindgen_test]
    fn test_author_name_falls_back() {
        assert_eq!(author_name(&notion_user(Some("Jane Doe"))), "Jane Doe");
        assert_eq!(author_name(&notion_user(Some("  "))), "Someone");
        assert_eq!(author_name(&notion_user(None)), "Someone");
    }

    #[wasm_bindgen_test]
    fn test_block_type_label() {
        assert_eq!(block_type_label("paragraph"), None);
        assert_eq!(block_type_label("table_row"), Some("Table row".to_string()));
        assert_eq!(block_type_label("to_do"), Some("To do".to_string()));
    }

    #[wasm_bindgen_test]
    fn test_render_html_highlights_the_user_mention() {
        let rich_text: Vec<RichText> = serde_json::from_value(serde_json::json!([{
            "type": "mention",
            "mention": { "type": "user", "user": { "object": "user", "id": "user-1" } },
            "annotations": {
                "bold": false, "italic": false, "strikethrough": false,
                "underline": false, "code": false, "color": "default"
            },
            "plain_text": "@David",
            "href": null
        }]))
        .unwrap();

        assert_eq!(
            render_html(&rich_text, Some("user-1")),
            "<span class=\"text-warning\">@David</span>"
        );
        assert_eq!(
            render_html(&rich_text, None),
            "<span class=\"text-primary\">@David</span>"
        );
    }
}
