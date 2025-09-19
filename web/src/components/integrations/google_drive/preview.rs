#![allow(non_snake_case)]

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsArrowUpRightSquare, Icon};
use url::Url;

use universal_inbox::{
    notification::NotificationWithTask,
    third_party::integrations::google_drive::{
        GoogleDriveComment, GoogleDriveCommentAuthor, GoogleDriveCommentReply,
    },
    HasHtmlUrl,
};

use crate::components::{
    integrations::google_drive::icons::GoogleDriveFile, markdown::Markdown, MessageHeader,
};

#[component]
pub fn GoogleDriveCommentPreview(
    notification: ReadOnlySignal<NotificationWithTask>,
    google_drive_comment: ReadOnlySignal<GoogleDriveComment>,
    expand_details: ReadOnlySignal<bool>,
) -> Element {
    let mut show_all_replies = use_signal(|| false);
    let _ = use_resource(move || async move {
        *show_all_replies.write() = expand_details();
    });
    let link = notification().get_html_url();
    let document_icon_style = if google_drive_comment().resolved {
        "text-green-500"
    } else {
        "text-blue-500"
    };

    let replies = google_drive_comment().replies;
    let first_unread_reply_index = replies
        .iter()
        .position(|reply| reply.modified_time >= notification().updated_at)
        .unwrap_or(replies.len());
    let (read_replies, unread_replies) = replies.split_at(first_unread_reply_index);
    let invisible_read_reply = match first_unread_reply_index {
        0 => None,
        1 => Some("1 hidden reply...".to_string()),
        n => Some(format!("{n} hidden replies...")),
    };
    let unread_reply = match unread_replies.len() {
        0 => None,
        1 => Some("1 unread reply".to_string()),
        n => Some(format!("{n} unread replies")),
    };

    rsx! {
        div {
            class: "flex flex-col gap-2 w-full h-full",

            h3 {
                class: "flex items-center gap-2 text-base",

                GoogleDriveFile {
                    class: "flex-none h-5 w-5 {document_icon_style}",
                    mime_type: "{google_drive_comment().file_mime_type}",
                }
                a {
                    class: "flex items-center",
                    href: "{link}",
                    target: "_blank",
                    "{notification().title}"
                    Icon { class: "h-5 w-5 min-w-5 text-base-content/50 p-1", icon: BsArrowUpRightSquare }
                }
            }

            if let Some(quoted_content) = google_drive_comment().quoted_file_content {
                div {
                    class: "border-l-4 border-base-300 pl-3 mb-3 text-base-content/70 italic",
                    "{quoted_content}"
                }
            }

            div {
                id: "notification-preview-details",
                class: "card w-full bg-base-200 h-full overflow-y-auto scroll-y-auto",

                div {
                    class: "card-body p-2 flex flex-col gap-2",

                    GoogleDriveCommentDisplay { comment: google_drive_comment },

                    if !show_all_replies() {
                        if let Some(invisible_read_reply) = invisible_read_reply {
                            div {
                                class: "divider divider-primary text-xs",
                                a {
                                    class: "link link-hover link-primary",
                                    onclick: move |_| { *show_all_replies.write() = true; },
                                    "{invisible_read_reply}"
                                }
                            }
                        }
                    } else {
                        for reply in read_replies {
                            GoogleDriveCommentReplyDisplay { reply: reply.clone() }
                        }
                    }

                    if let Some(unread_reply) = unread_reply {
                        div {
                            class: "divider divider-primary my-0 text-xs text-primary",
                            "{unread_reply}"
                        }
                        for reply in unread_replies {
                            GoogleDriveCommentReplyDisplay { reply: reply.clone() }
                        }
                    }

                }
            }
        }
    }
}

#[component]
fn GoogleDriveCommentDisplay(comment: ReadOnlySignal<GoogleDriveComment>) -> Element {
    rsx! {
        CommentDisplay {
            modified_time: comment().modified_time,
            author: comment().author,
            html_content: comment().html_content,
            content: comment().content,
        }
    }
}

#[component]
fn GoogleDriveCommentReplyDisplay(reply: ReadOnlySignal<GoogleDriveCommentReply>) -> Element {
    rsx! {
        CommentDisplay {
            modified_time: reply().modified_time,
            author: reply().author,
            html_content: reply().html_content,
            content: reply().content,
        }
    }
}

#[component]
fn CommentDisplay(
    modified_time: ReadOnlySignal<DateTime<Utc>>,
    author: ReadOnlySignal<GoogleDriveCommentAuthor>,
    html_content: ReadOnlySignal<Option<String>>,
    content: ReadOnlySignal<String>,
) -> Element {
    let avatar_url: Option<Url> = author()
        .photo_link
        .and_then(|link| link.parse::<Url>().ok());
    let cleaned_html_content =
        use_memo(move || html_content().as_ref().map(|html| ammonia::clean(html)));

    rsx! {
        div {
            class: "flex flex-col gap-0",
            div {
                class: "flex items-center gap-2 text-xs text-base-content/50",

                MessageHeader {
                    user_name: "{author().display_name}",
                    avatar_url,
                    display_name: true,
                    sent_at: Some(modified_time()),
                    date_class: "text-base-content/75",
                }
            }

            div {
                class: "prose prose-sm prose-table:text-sm",
                if let Some(cleaned_html_content) = cleaned_html_content() {
                    span { dangerous_inner_html: "{cleaned_html_content}" }
                } else {
                    Markdown { text: "{content()}", class: "w-full max-w-full" }
                }
            }
        }
    }
}
