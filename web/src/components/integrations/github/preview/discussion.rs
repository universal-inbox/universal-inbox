#![allow(non_snake_case)]

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use dioxus::prelude::*;

use universal_inbox::{
    third_party::integrations::github::{
        GithubActor, GithubDiscussion, GithubDiscussionComment, GithubDiscussionThreadComment,
    },
    utils::emoji::replace_emoji_code_with_emoji,
};

use crate::{
    components::{
        TagList, UserWithAvatar,
        integrations::github::{
            GithubActorDisplay, get_github_actor_name_and_url,
            preview::read_state::{Segment, is_read, read_prefix_len, segments},
        },
        preview_card_header::PreviewCardHeader,
        thread::{
            ExpandDivider, LATEST_READ_ANCHOR_ID, NewMessagesDivider, Thread, ThreadChildren,
            ThreadItem, UnreadDivider, use_thread_collapse,
        },
        ui::{
            Card, CardVariant, MetadataGrid, MetadataItem, Tag as UiTag, TagVariant,
            thread_message::ThreadedMessage,
        },
    },
    utils::{format_elapsed_time, sanitize_html},
};

const EXPAND_ICON: &str = "icon-[lucide--unfold-vertical]";

type CommentSegment = Segment<GithubDiscussionThreadComment>;

#[component]
pub fn GithubDiscussionPreview(
    github_discussion: ReadSignal<GithubDiscussion>,
    title: ReadSignal<String>,
    last_read_at: ReadSignal<Option<DateTime<Utc>>>,
    expand_details: ReadSignal<bool>,
) -> Element {
    // Drives the original post visibility; each comment card scrolls and
    // collapses its own replies.
    let collapse = use_thread_collapse("notification-preview-details", expand_details, move || {
        (github_discussion().id, false)
    });
    let mut show_body = collapse.show_root;
    // Runs of read comments the user expanded, keyed by their first comment id
    let mut expanded_read_comments = use_signal(HashSet::<String>::new);
    // Collapse them again when the expand shortcut toggles or the discussion changes
    use_effect(move || {
        let _ = expand_details();
        let _ = github_discussion().id;
        expanded_read_comments.write().clear();
    });

    let discussion = github_discussion();
    let is_answered = discussion.is_answered();

    let (state_variant, state_label) = if is_answered {
        (TagVariant::Success, "Answered")
    } else {
        (TagVariant::Info, "Open")
    };

    let created_ago = format_elapsed_time(discussion.created_at);

    let identifier = format!("#{}", discussion.number);
    let repo_name = discussion.repository.name_with_owner.clone();
    let repo_url = discussion.repository.url.clone();
    let discussion_url = discussion.url.clone();
    let author = discussion.author.clone();

    let last_read_at = last_read_at();
    let body_hidden = is_read(discussion.created_at, last_read_at) && !show_body();
    let anchor_comment_id = scroll_anchor_comment_id(&discussion, last_read_at);
    let segments = comment_segments(&discussion, last_read_at);
    let show_all_read_comments = expand_details();

    rsx! {
        div {
            class: "flex flex-col w-full h-full",

            PreviewCardHeader {
                brand_icon: rsx! { span { class: "icon-[lucide--message-square] size-4" } },
                title: title(),
                identifier: Some(identifier),
                subline: rsx! {
                    if let Some(actor) = author {
                        span { "Opened by" }
                        {
                            let (name, url) = get_github_actor_name_and_url(actor);
                            rsx! {
                                UserWithAvatar {
                                    user_name: name,
                                    avatar_url: Some(Some(url)),
                                    display_name: true,
                                    class: "text-[11px]",
                                }
                            }
                        }
                        span { class: "sep", "·" }
                        span { "{created_ago} ago" }
                    }
                }
            }

            div {
                id: "notification-preview-details",
                class: "flex flex-col gap-2 w-full h-full overflow-y-auto scroll-y-auto p-3",

                Card {
                    variant: CardVariant::Default,

                    MetadataGrid {
                        MetadataItem {
                            label: "Repository".to_string(),
                            value: rsx! {
                                a {
                                    href: "{repo_url}",
                                    target: "_blank",
                                    rel: "noopener noreferrer",
                                    "{repo_name}"
                                }
                                a {
                                    href: "{discussion_url}",
                                    target: "_blank",
                                    rel: "noopener noreferrer",
                                    "#{discussion.number}"
                                }
                            },
                        }

                        MetadataItem {
                            label: "State".to_string(),
                            value: rsx! {
                                UiTag { variant: state_variant, "{state_label}" }
                            },
                        }

                        if let Some(category) = &discussion.category {
                            MetadataItem {
                                label: "Category".to_string(),
                                value: rsx! {
                                    if let Some(emoji_glyph) = category.emoji.as_deref().and_then(replace_emoji_code_with_emoji) {
                                        span { "{emoji_glyph}" }
                                    }
                                    span { "{category.name}" }
                                },
                            }
                        }

                        MetadataItem {
                            label: "Updated".to_string(),
                            value: rsx! {
                                span { "{format_elapsed_time(discussion.updated_at)} ago" }
                            },
                        }
                    }

                    if !discussion.labels.is_empty() {
                        TagList {
                            tags: discussion
                                .labels
                                .iter()
                                .map(|label| label.clone().into())
                                .collect()
                        }
                    }
                }

                if body_hidden {
                    ExpandDivider {
                        icon: "icon-[lucide--arrow-up-to-line]",
                        label: "Show original post",
                        onclick: move |_| { *show_body.write() = true; },
                    }
                } else {
                    Card {
                        variant: CardVariant::Default,
                        div {
                            class: "w-full max-w-full prose prose-sm dark:prose-invert",
                            dangerous_inner_html: sanitize_html(&discussion.body)
                        }
                    }
                }

                if let (Some(answer), Some(actor)) = (&discussion.answer, &discussion.answer_chosen_by) {
                    div {
                        class: "bg-ui-success-subtle border border-ui-border rounded-ui-lg p-3 mb-2.5",

                        div {
                            class: "flex items-center gap-2 text-xs mb-2",
                            span {
                                class: "icon-[lucide--check-circle] size-4",
                                style: "color: var(--ui-success);",
                            }
                            span {
                                style: "color: var(--ui-success); font-weight: 600;",
                                "Accepted answer by"
                            }
                            GithubActorDisplay { actor: actor.clone(), display_name: true }
                        }
                        div {
                            class: "w-full max-w-full prose prose-sm dark:prose-invert",
                            dangerous_inner_html: sanitize_html(&answer.body)
                        }
                    }
                }

                for segment in segments {
                    match segment {
                        CommentSegment::Comment(thread) => rsx! {
                            DiscussionCommentCard {
                                key: "{thread.id}",
                                is_scroll_anchor: anchor_comment_id.as_deref() == Some(thread.id.as_str()),
                                thread: *thread,
                                last_read_at,
                                expand_details,
                            }
                        },
                        CommentSegment::ReadComments(threads) => {
                            let group_key = threads[0].id.clone();
                            if show_all_read_comments || expanded_read_comments.read().contains(&group_key) {
                                rsx! {
                                    for thread in threads {
                                        DiscussionCommentCard {
                                            key: "{thread.id}",
                                            is_scroll_anchor: false,
                                            initially_expanded: true,
                                            thread,
                                            last_read_at,
                                            expand_details,
                                        }
                                    }
                                }
                            } else {
                                rsx! {
                                    ExpandDivider {
                                        key: "{group_key}",
                                        icon: EXPAND_ICON,
                                        label: format!("Show {} read comments", threads.len()),
                                        onclick: move |_| {
                                            expanded_read_comments.write().insert(group_key.clone());
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A comment and its replies, in their own card:
/// - fully read: a single "Show comment from AUTHOR · N replies" control;
/// - comment and some replies read: "Show comment from AUTHOR and N earlier
///   replies", the latest read reply, then the unread replies;
/// - comment read, no reply read: the comment, then the unread replies;
/// - comment unread: everything, below a "NEW COMMENT" divider.
#[component]
fn DiscussionCommentCard(
    thread: ReadSignal<GithubDiscussionThreadComment>,
    last_read_at: ReadSignal<Option<DateTime<Utc>>>,
    is_scroll_anchor: ReadSignal<bool>,
    initially_expanded: Option<bool>,
    expand_details: ReadSignal<bool>,
) -> Element {
    let collapse = use_thread_collapse("notification-preview-details", expand_details, move || {
        (thread().id, is_scroll_anchor())
    });
    let mut show_all = collapse.show_all;

    let thread = thread();
    let last_read_at = last_read_at();
    let is_scroll_anchor = is_scroll_anchor();
    let expanded = show_all() || initially_expanded.unwrap_or_default();

    let read_count = read_reply_count(&thread, last_read_at);
    let replies_count = thread.replies.len();
    let unread_count = replies_count - read_count;
    let comment_read = is_read(thread.comment.created_at, last_read_at);
    let author = comment_author_name(&thread.comment);

    let mut read_replies = thread.replies[..read_count].to_vec();
    let unread_replies = thread.replies[read_count..].to_vec();
    let state = if !comment_read {
        CardState::New
    } else if expanded || (read_count == 0 && unread_count > 0) {
        CardState::Expanded
    } else if unread_count == 0 {
        CardState::Read
    } else {
        CardState::PartiallyRead
    };
    let latest_read_reply = if state == CardState::PartiallyRead {
        read_replies.pop()
    } else {
        None
    };
    // Without a latest read reply, the whole card is the scroll anchor
    let card_id =
        (is_scroll_anchor && latest_read_reply.is_none()).then_some(LATEST_READ_ANCHOR_ID);
    let expand = move |_| {
        *show_all.write() = true;
    };

    rsx! {
        div {
            id: card_id,
            Card {
                variant: CardVariant::Default,

                Thread {
                    match state {
                        CardState::Read => rsx! {
                            ExpandDivider {
                                icon: EXPAND_ICON,
                                label: match replies_count {
                                    0 => format!("Show comment from {author}"),
                                    1 => format!("Show comment from {author} · 1 reply"),
                                    n => format!("Show comment from {author} · {n} replies"),
                                },
                                onclick: expand,
                            }
                        },
                        CardState::PartiallyRead => rsx! {
                            ExpandDivider {
                                icon: EXPAND_ICON,
                                label: match read_count - 1 {
                                    0 => format!("Show comment from {author}"),
                                    1 => format!("Show comment from {author} and 1 earlier reply"),
                                    n => format!("Show comment from {author} and {n} earlier replies"),
                                },
                                onclick: expand,
                            }
                            if let Some(reply) = latest_read_reply {
                                div {
                                    id: is_scroll_anchor.then_some(LATEST_READ_ANCHOR_ID),
                                    DiscussionReply { reply }
                                }
                            }
                            NewMessagesDivider { unread_count, singular: "reply", plural: "replies" }
                            for reply in unread_replies {
                                DiscussionReply { reply }
                            }
                        },
                        CardState::Expanded => rsx! {
                            ThreadItem {
                                DiscussionMessage { comment: thread.comment.clone(), is_answer: thread.is_answer }
                            }
                            for reply in read_replies {
                                DiscussionReply { reply }
                            }
                            if unread_count > 0 {
                                NewMessagesDivider { unread_count, singular: "reply", plural: "replies" }
                            }
                            for reply in unread_replies {
                                DiscussionReply { reply }
                            }
                        },
                        CardState::New => rsx! {
                            UnreadDivider {
                                label: match replies_count {
                                    0 => "NEW COMMENT".to_string(),
                                    1 => "NEW COMMENT · 1 REPLY".to_string(),
                                    n => format!("NEW COMMENT · {n} REPLIES"),
                                },
                            }
                            ThreadItem {
                                DiscussionMessage { comment: thread.comment.clone(), is_answer: thread.is_answer }
                            }
                            for reply in thread.replies.iter().cloned() {
                                DiscussionReply { reply }
                            }
                        },
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum CardState {
    /// Comment and replies read, all collapsed
    Read,
    /// Comment and some replies read, some replies unread
    PartiallyRead,
    /// Everything shown (unread replies after a divider)
    Expanded,
    /// The comment itself is unread, so are its replies
    New,
}

#[component]
fn DiscussionReply(reply: ReadSignal<GithubDiscussionComment>) -> Element {
    rsx! {
        ThreadItem {
            ThreadChildren {
                DiscussionMessage { comment: reply(), is_answer: false }
            }
        }
    }
}

#[component]
fn DiscussionMessage(
    comment: ReadSignal<GithubDiscussionComment>,
    is_answer: ReadSignal<bool>,
) -> Element {
    let comment = comment();
    let (author_name, author_avatar_url) = match comment.author {
        Some(actor) => author_name_and_avatar(actor),
        None => ("Unknown".to_string(), None),
    };

    rsx! {
        ThreadedMessage {
            author_name,
            author_avatar_url,
            sent_at: Some(comment.created_at),
            metadata: if is_answer() {
                Some(rsx! {
                    UiTag {
                        variant: TagVariant::Success,
                        span { class: "icon-[lucide--check-circle] size-3" }
                        "Answer"
                    }
                })
            } else {
                None
            },
            body: rsx! {
                div {
                    class: "w-full max-w-full prose prose-sm dark:prose-invert",
                    dangerous_inner_html: sanitize_html(&comment.body)
                }
            },
        }
    }
}

fn author_name_and_avatar(actor: GithubActor) -> (String, Option<url::Url>) {
    let (name, url) = get_github_actor_name_and_url(actor);
    (name, Some(url))
}

fn comment_author_name(comment: &GithubDiscussionComment) -> String {
    comment
        .author
        .clone()
        .map(|actor| get_github_actor_name_and_url(actor).0)
        .unwrap_or_else(|| "Unknown".to_string())
}

/// Number of leading read replies (replies are oldest first).
fn read_reply_count(
    thread: &GithubDiscussionThreadComment,
    last_read_at: Option<DateTime<Utc>>,
) -> usize {
    read_prefix_len(&thread.replies, |reply| reply.created_at, last_read_at)
}

fn has_unread(thread: &GithubDiscussionThreadComment, last_read_at: Option<DateTime<Utc>>) -> bool {
    !is_read(thread.comment.created_at, last_read_at)
        || read_reply_count(thread, last_read_at) < thread.replies.len()
}

fn comment_segments(
    discussion: &GithubDiscussion,
    last_read_at: Option<DateTime<Utc>>,
) -> Vec<CommentSegment> {
    segments(&discussion.comments, |thread| {
        has_unread(thread, last_read_at)
    })
}

/// The comment the preview scrolls to when opened: the first one with unread
/// messages, when something before it was already read.
fn scroll_anchor_comment_id(
    discussion: &GithubDiscussion,
    last_read_at: Option<DateTime<Utc>>,
) -> Option<String> {
    if !is_read(discussion.created_at, last_read_at) {
        return None;
    }
    discussion
        .comments
        .iter()
        .find(|thread| has_unread(thread, last_read_at))
        .map(|thread| thread.id.clone())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use universal_inbox::third_party::integrations::github::{
        GithubDiscussionThreadComment, GithubRepositorySummary,
    };
    use wasm_bindgen_test::*;

    use super::*;

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 1, hour, 0, 0).unwrap()
    }

    fn comment(hour: u32) -> GithubDiscussionComment {
        GithubDiscussionComment {
            url: "https://github.com/o/r/discussions/1".parse().unwrap(),
            body: format!("<p>{hour}</p>"),
            created_at: at(hour),
            author: None,
        }
    }

    fn discussion(comments: Vec<GithubDiscussionThreadComment>) -> GithubDiscussion {
        GithubDiscussion {
            id: "D_1".to_string(),
            number: 1,
            url: "https://github.com/o/r/discussions/1".parse().unwrap(),
            title: "title".to_string(),
            body: "<p>body</p>".to_string(),
            repository: GithubRepositorySummary {
                name_with_owner: "o/r".to_string(),
                url: "https://github.com/o/r".parse().unwrap(),
            },
            state_reason: None,
            closed_at: None,
            created_at: at(1),
            updated_at: at(9),
            labels: vec![],
            comments_count: comments.len() as i64,
            author: None,
            answer_chosen_at: None,
            answer_chosen_by: None,
            answer: None,
            category: None,
            comments,
        }
    }

    fn thread(hour: u32, reply_hours: &[u32]) -> GithubDiscussionThreadComment {
        GithubDiscussionThreadComment {
            id: format!("DC_{hour}"),
            comment: comment(hour),
            is_answer: false,
            replies: reply_hours.iter().map(|h| comment(*h)).collect(),
            replies_count: reply_hours.len() as i64,
        }
    }

    #[wasm_bindgen_test]
    fn read_replies_are_counted_per_thread() {
        let t = thread(2, &[3, 6]);
        assert_eq!(read_reply_count(&t, None), 0);
        assert_eq!(read_reply_count(&t, Some(at(4))), 1);
        assert_eq!(read_reply_count(&t, Some(at(9))), 2);
        assert!(has_unread(&t, Some(at(4))));
        assert!(!has_unread(&t, Some(at(9))));
        // An unread comment is unread even without replies
        assert!(has_unread(&thread(5, &[]), Some(at(4))));
    }

    #[wasm_bindgen_test]
    fn scroll_anchor_is_first_thread_with_unread_messages() {
        let d = discussion(vec![thread(2, &[3]), thread(4, &[5, 7]), thread(6, &[])]);
        // A new reply in the second thread comes before the newer third comment
        assert_eq!(
            scroll_anchor_comment_id(&d, Some(at(6))),
            Some("DC_4".to_string())
        );
        assert_eq!(scroll_anchor_comment_id(&d, Some(at(9))), None);
        // Nothing read: the preview stays at the top
        assert_eq!(scroll_anchor_comment_id(&d, None), None);
    }

    #[wasm_bindgen_test]
    fn consecutive_read_comments_are_grouped() {
        let ids = |segments: Vec<CommentSegment>| -> Vec<String> {
            segments
                .into_iter()
                .map(|segment| match segment {
                    CommentSegment::Comment(t) => t.id,
                    CommentSegment::ReadComments(ts) => {
                        ts.into_iter().map(|t| t.id).collect::<Vec<_>>().join("+")
                    }
                })
                .collect()
        };
        let d = discussion(vec![
            thread(2, &[]),
            thread(3, &[4]),
            thread(5, &[8]),
            thread(6, &[]),
            thread(7, &[]),
        ]);
        // DC_5 has an unread reply, DC_7 is unread: DC_6 alone stays a card
        assert_eq!(
            ids(comment_segments(&d, Some(at(6)))),
            vec!["DC_2+DC_3", "DC_5", "DC_6", "DC_7"]
        );
        assert_eq!(
            ids(comment_segments(&d, Some(at(9)))),
            vec!["DC_2+DC_3+DC_5+DC_6+DC_7"]
        );
        assert_eq!(comment_segments(&d, None).len(), 5);
    }
}
