#![allow(non_snake_case)]

use dioxus::prelude::*;
use url::Url;

use universal_inbox::{
    notification::NotificationWithTask,
    third_party::integrations::google_drive::{GoogleDriveComment, GoogleDriveCommentReply},
};

use crate::{
    components::{
        markdown::Markdown,
        preview_card_header::PreviewCardHeader,
        thread::{Thread, ThreadDivider, ThreadItem},
        threaded_message::ThreadedMessage,
        ui::{Tag, TagVariant},
    },
    utils::format_elapsed_time,
};

#[component]
pub fn GoogleDriveCommentPreview(
    notification: ReadSignal<NotificationWithTask>,
    google_drive_comment: ReadSignal<GoogleDriveComment>,
    expand_details: ReadSignal<bool>,
) -> Element {
    let mut show_all_replies = use_signal(|| false);
    let _resource = use_resource(move || async move {
        *show_all_replies.write() = expand_details();
    });

    let comment = google_drive_comment();
    let replies = comment.replies.clone();
    // PRESERVE VERBATIM the existing hidden/unread divider computation.
    let first_unread_reply_index = replies
        .iter()
        .position(|reply| reply.modified_time >= notification().updated_at)
        .unwrap_or(replies.len());
    let (read_replies, unread_replies) = replies.split_at(first_unread_reply_index);
    let read_replies = read_replies.to_vec();
    let unread_replies = unread_replies.to_vec();
    let invisible_read_reply = match first_unread_reply_index {
        0 => None,
        1 => Some("1 hidden reply...".to_string()),
        n => Some(format!("{n} hidden replies...")),
    };
    let unread_reply_label = match unread_replies.len() {
        0 => None,
        1 => Some("1 unread reply".to_string()),
        n => Some(format!("{n} unread replies")),
    };

    let author_display = comment.author.display_name.clone();
    let comment_age = format_elapsed_time(comment.modified_time);
    let file_name = comment.file_name.clone();
    let is_resolved = comment.resolved.unwrap_or(false);

    let header_author = author_display.clone();
    let header_age = comment_age.clone();

    rsx! {
        div {
            class: "flex flex-col w-full h-full",

            PreviewCardHeader {
                brand_icon: rsx! { span { class: "icon-[lucide--file] size-4" } },
                title: file_name.clone(),
                subline: rsx! {
                    span { class: "icon-[lucide--message-square] size-3" }
                    span { "Comment by" }
                    span {
                        style: "color: var(--ui-base-content); font-weight: 500;",
                        "@{header_author}"
                    }
                    span { class: "sep", "·" }
                    span { "{header_age} ago" }
                    if is_resolved {
                        span { class: "sep", "·" }
                        Tag { variant: TagVariant::Muted, "Resolved" }
                    }
                }
            }

            div {
                id: "notification-preview-details",
                class: "flex flex-col gap-2 w-full h-full overflow-y-auto scroll-y-auto p-3",

                // Anchored doc text — pull-quote with left rule, distinct from comment body.
                if let Some(quoted_content) = comment.quoted_file_content.clone() {
                    div {
                        class: "border-l-[3px] border-solid border-ui-primary-light bg-ui-primary-subtle px-3 py-2 text-[12.5px] italic rounded-r-ui-sm text-ui-base-content",
                        "{quoted_content}"
                    }
                }

                div {
                    class: "preview-card",

                    Thread {
                        // Parent comment.
                        ThreadItem {
                            DriveCommentRow {
                                author_name: comment.author.display_name.clone(),
                                avatar_link: comment.author.photo_link.clone(),
                                modified_time: comment.modified_time,
                                html_content: comment.html_content.clone(),
                                content: comment.content.clone(),
                                dimmed: is_resolved,
                            }
                        }

                        // Replies render flat — same vertical alignment as the
                        // parent (no `ThreadChildren` indent), matching the
                        // Slack thread preview.
                        if !show_all_replies() {
                            if let Some(invisible_read_reply) = invisible_read_reply {
                                ThreadDivider {
                                    a {
                                        onclick: move |_| { *show_all_replies.write() = true; },
                                        "{invisible_read_reply}"
                                    }
                                }
                            }
                        } else {
                            for reply in read_replies.iter().cloned() {
                                GoogleDriveCommentReplyDisplay { reply, dimmed: is_resolved }
                            }
                        }

                        if let Some(unread_reply_label) = unread_reply_label {
                            ThreadDivider {
                                unread: true,
                                "{unread_reply_label}"
                            }
                            for reply in unread_replies.iter().cloned() {
                                GoogleDriveCommentReplyDisplay { reply, dimmed: is_resolved }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn GoogleDriveCommentReplyDisplay(
    reply: ReadSignal<GoogleDriveCommentReply>,
    dimmed: bool,
) -> Element {
    let r = reply();
    let action_label: Option<(&'static str, &'static str)> =
        r.action.as_deref().and_then(|a| match a {
            "resolve" => Some((
                "icon-[lucide--check-circle-2]",
                "marked this thread as resolved",
            )),
            "reopen" => Some(("icon-[lucide--rotate-ccw]", "reopened this thread")),
            _ => None,
        });

    let avatar_url: Option<Url> = r
        .author
        .photo_link
        .clone()
        .and_then(|link| link.parse::<Url>().ok());

    rsx! {
        ThreadItem {
            if let Some((icon_class, label)) = action_label {
                ThreadedMessage {
                    author_name: r.author.display_name.clone(),
                    author_avatar_url: avatar_url,
                    sent_at: Some(r.modified_time),
                    dimmed,
                    body: rsx! {
                        div {
                            class: "flex items-center gap-2 text-sm italic",
                            style: "color: var(--ui-base-content-muted);",
                            span { class: "{icon_class} size-3.5" }
                            span { "{label}" }
                        }
                    },
                }
            } else {
                DriveCommentRow {
                    author_name: r.author.display_name.clone(),
                    avatar_link: r.author.photo_link.clone(),
                    modified_time: r.modified_time,
                    html_content: r.html_content,
                    content: r.content,
                    dimmed,
                }
            }
        }
    }
}

#[component]
fn DriveCommentRow(
    author_name: String,
    avatar_link: Option<String>,
    modified_time: chrono::DateTime<chrono::Utc>,
    html_content: Option<String>,
    content: String,
    dimmed: bool,
) -> Element {
    let avatar_url: Option<Url> = avatar_link.and_then(|link| link.parse::<Url>().ok());
    let cleaned_html_content = html_content.as_ref().map(|html| {
        ammonia::Builder::default()
            .set_tag_attribute_value("a", "target", "_blank")
            .clean(html)
            .to_string()
    });
    let body_text = content.clone();

    rsx! {
        ThreadedMessage {
            author_name,
            author_avatar_url: avatar_url,
            sent_at: Some(modified_time),
            dimmed,
            body: rsx! {
                div {
                    class: "prose prose-sm prose-table:text-sm",
                    if let Some(cleaned_html_content) = cleaned_html_content {
                        span { dangerous_inner_html: "{cleaned_html_content}" }
                    } else {
                        Markdown { text: "{body_text}", class: "w-full max-w-full" }
                    }
                }
            },
        }
    }
}
