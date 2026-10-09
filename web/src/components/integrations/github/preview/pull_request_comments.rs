#![allow(non_snake_case)]

//! The conversation of a pull request: its description, then its comments,
//! reviews and review threads in chronological order. Like discussions,
//! already read messages are collapsed.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use url::Url;

use universal_inbox::third_party::integrations::github::{
    GithubActor, GithubPullRequest, GithubPullRequestReviewState,
};

use crate::{
    components::{
        integrations::github::{
            get_github_actor_name_and_url,
            preview::read_state::{Segment, is_read, read_prefix_len, segments},
        },
        thread::{
            ExpandDivider, LATEST_READ_ANCHOR_ID, NewMessagesDivider, Thread, ThreadChildren,
            ThreadItem, UnreadDivider, use_thread_collapse,
        },
        ui::{Card, CardVariant, Tag as UiTag, TagVariant, thread_message::ThreadedMessage},
    },
    utils::sanitize_html,
};

const EXPAND_ICON: &str = "icon-[lucide--unfold-vertical]";
/// Lines of code shown above the commented line(s), as GitHub does.
const DIFF_CONTEXT_LINES: usize = 3;

/// A comment, a review or a review thread of the pull request.
#[derive(Clone, PartialEq, Debug)]
pub struct TimelineItem {
    pub key: String,
    pub kind: TimelineItemKind,
    /// Oldest first; the first one starts the thread.
    pub messages: Vec<TimelineMessage>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum TimelineItemKind {
    Comment,
    Review(GithubPullRequestReviewState),
    ReviewThread(ReviewThreadContext),
}

#[derive(Clone, PartialEq, Debug)]
pub struct ReviewThreadContext {
    pub path: String,
    pub diff_hunk: String,
    pub line: Option<i64>,
    pub start_line: Option<i64>,
    pub is_resolved: bool,
    pub is_outdated: bool,
}

#[derive(Clone, PartialEq, Debug)]
pub struct TimelineMessage {
    pub author: Option<GithubActor>,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub url: Url,
}

impl TimelineItem {
    fn created_at(&self) -> DateTime<Utc> {
        self.messages[0].created_at
    }

    fn read_count(&self, last_read_at: Option<DateTime<Utc>>) -> usize {
        read_prefix_len(&self.messages, |message| message.created_at, last_read_at)
    }

    fn has_unread(&self, last_read_at: Option<DateTime<Utc>>) -> bool {
        self.read_count(last_read_at) < self.messages.len()
    }
}

/// Comments, reviews with a body (the others only show in the reviewers
/// section) and review threads, oldest first.
pub fn build_timeline(pull_request: &GithubPullRequest) -> Vec<TimelineItem> {
    let comments = pull_request.comments.iter().map(|comment| TimelineItem {
        key: comment
            .id
            .clone()
            .unwrap_or_else(|| comment.url.to_string()),
        kind: TimelineItemKind::Comment,
        messages: vec![TimelineMessage {
            author: comment.author.clone(),
            body: comment.body.clone(),
            created_at: comment.created_at,
            url: comment.url.clone(),
        }],
    });
    let reviews = pull_request.reviews.iter().filter_map(|review| {
        if review.body.trim().is_empty() {
            return None;
        }
        // Items stored before these fields were fetched lack them
        let (Some(id), Some(url), Some(submitted_at)) =
            (&review.id, &review.url, review.submitted_at)
        else {
            return None;
        };
        Some(TimelineItem {
            key: id.clone(),
            kind: TimelineItemKind::Review(review.state),
            messages: vec![TimelineMessage {
                author: review.author.clone(),
                body: review.body.clone(),
                created_at: submitted_at,
                url: url.clone(),
            }],
        })
    });
    let review_threads = pull_request
        .review_threads
        .iter()
        .filter(|thread| !thread.comments.is_empty())
        .map(|thread| TimelineItem {
            key: thread.id.clone(),
            kind: TimelineItemKind::ReviewThread(ReviewThreadContext {
                path: thread.path.clone(),
                diff_hunk: thread.diff_hunk.clone(),
                line: thread.line,
                start_line: thread.start_line,
                is_resolved: thread.is_resolved,
                is_outdated: thread.is_outdated,
            }),
            messages: thread
                .comments
                .iter()
                .map(|comment| TimelineMessage {
                    author: comment.author.clone(),
                    body: comment.body.clone(),
                    created_at: comment.created_at,
                    url: comment.url.clone(),
                })
                .collect(),
        });

    let mut timeline: Vec<TimelineItem> = comments.chain(reviews).chain(review_threads).collect();
    timeline.sort_by_key(|item| item.created_at());
    timeline
}

/// The item the preview scrolls to when opened: the first one with unread
/// messages, when the description was already read.
fn scroll_anchor_key(
    pull_request: &GithubPullRequest,
    timeline: &[TimelineItem],
    last_read_at: Option<DateTime<Utc>>,
) -> Option<String> {
    if !is_read(pull_request.created_at, last_read_at) {
        return None;
    }
    timeline
        .iter()
        .find(|item| item.has_unread(last_read_at))
        .map(|item| item.key.clone())
}

#[component]
pub fn GithubPullRequestConversation(
    github_pull_request: ReadSignal<GithubPullRequest>,
    last_read_at: ReadSignal<Option<DateTime<Utc>>>,
    expand_details: ReadSignal<bool>,
) -> Element {
    // Drives the description visibility; each card scrolls and collapses
    // its own messages.
    let collapse = use_thread_collapse("notification-preview-details", expand_details, move || {
        (github_pull_request().id, false)
    });
    let mut show_body = collapse.show_root;
    // Runs of read items the user expanded, keyed by their first item key
    let mut expanded_read_items = use_signal(HashSet::<String>::new);
    // Collapse them again when the expand shortcut toggles or the pull request changes
    use_effect(move || {
        let _ = expand_details();
        let _ = github_pull_request().id;
        expanded_read_items.write().clear();
    });

    let pull_request = github_pull_request();
    let last_read_at = last_read_at();
    let body_hidden = is_read(pull_request.created_at, last_read_at) && !show_body();
    let timeline = build_timeline(&pull_request);
    let anchor_key = scroll_anchor_key(&pull_request, &timeline, last_read_at);
    let segments = segments(&timeline, |item| item.has_unread(last_read_at));
    let show_all_read_items = expand_details();

    rsx! {
        if !pull_request.body.is_empty() {
            if body_hidden {
                ExpandDivider {
                    icon: "icon-[lucide--arrow-up-to-line]",
                    label: "Show description",
                    onclick: move |_| { *show_body.write() = true; },
                }
            } else {
                Card {
                    variant: CardVariant::Default,
                    div {
                        class: "w-full max-w-full prose prose-sm dark:prose-invert",
                        dangerous_inner_html: sanitize_html(&pull_request.body)
                    }
                }
            }
        }

        for segment in segments {
            match segment {
                Segment::Comment(item) => rsx! {
                    TimelineCard {
                        key: "{item.key}",
                        is_scroll_anchor: anchor_key.as_deref() == Some(item.key.as_str()),
                        item: *item,
                        last_read_at,
                        expand_details,
                    }
                },
                Segment::ReadComments(items) => {
                    let group_key = items[0].key.clone();
                    if show_all_read_items || expanded_read_items.read().contains(&group_key) {
                        rsx! {
                            for item in items {
                                TimelineCard {
                                    key: "{item.key}",
                                    is_scroll_anchor: false,
                                    initially_expanded: true,
                                    item,
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
                                label: format!("Show {} read comments", items.len()),
                                onclick: move |_| {
                                    expanded_read_items.write().insert(group_key.clone());
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum CardState {
    /// Everything read, collapsed (resolved review threads always are)
    Read,
    /// Thread start and some messages read, some unread
    PartiallyRead,
    /// Everything shown (unread messages after a divider)
    Expanded,
    /// The thread start itself is unread, so is everything else
    New,
}

fn card_state(
    item: &TimelineItem,
    last_read_at: Option<DateTime<Utc>>,
    expanded: bool,
) -> CardState {
    let read_count = item.read_count(last_read_at);
    let is_resolved = matches!(
        &item.kind,
        TimelineItemKind::ReviewThread(context) if context.is_resolved
    );
    if read_count == 0 && !is_resolved {
        CardState::New
    } else if expanded {
        CardState::Expanded
    } else if is_resolved || read_count == item.messages.len() {
        CardState::Read
    } else if read_count == 1 {
        CardState::Expanded
    } else {
        CardState::PartiallyRead
    }
}

/// A comment, review or review thread, in its own card:
/// - fully read (or resolved thread): a single "Show ..." control;
/// - thread start and some replies read: "Show comment from AUTHOR and N
///   earlier replies", the latest read reply, then the unread replies;
/// - thread start read, no reply read: the thread start, then the unread replies;
/// - thread start unread: everything, below a "NEW COMMENT" divider.
#[component]
fn TimelineCard(
    item: ReadSignal<TimelineItem>,
    last_read_at: ReadSignal<Option<DateTime<Utc>>>,
    is_scroll_anchor: ReadSignal<bool>,
    initially_expanded: Option<bool>,
    expand_details: ReadSignal<bool>,
) -> Element {
    let collapse = use_thread_collapse("notification-preview-details", expand_details, move || {
        (item().key, is_scroll_anchor())
    });
    let mut show_all = collapse.show_all;

    let item = item();
    let last_read_at = last_read_at();
    let is_scroll_anchor = is_scroll_anchor();
    let expanded = show_all() || initially_expanded.unwrap_or_default();

    let state = card_state(&item, last_read_at, expanded);
    let read_count = item.read_count(last_read_at);
    let unread_count = item.messages.len() - read_count;
    let replies_count = item.messages.len() - 1;
    let author = author_name(&item.messages[0].author);
    let first_message = item.messages[0].clone();
    // Read replies, then unread ones (the thread start is never a reply)
    let mut read_replies = item.messages[1..read_count.max(1)].to_vec();
    let unread_replies = item.messages[read_count.max(1)..].to_vec();
    let latest_read_reply = if state == CardState::PartiallyRead {
        read_replies.pop()
    } else {
        None
    };
    let card_id =
        (is_scroll_anchor && latest_read_reply.is_none()).then_some(LATEST_READ_ANCHOR_ID);
    let expand = move |_| {
        *show_all.write() = true;
    };
    let kind = item.kind.clone();
    let review_state = match &item.kind {
        TimelineItemKind::Review(state) => Some(*state),
        _ => None,
    };
    let thread_context = match &item.kind {
        TimelineItemKind::ReviewThread(context) => Some(context.clone()),
        _ => None,
    };

    rsx! {
        div {
            id: card_id,
            Card {
                variant: CardVariant::Default,

                if state != CardState::Read {
                    if let Some(context) = thread_context.clone() {
                        ReviewThreadHeader { context, url: first_message.url.clone() }
                    }
                }

                Thread {
                    match state {
                        CardState::Read => rsx! {
                            ExpandDivider {
                                icon: EXPAND_ICON,
                                label: collapsed_label(&kind, &author, replies_count, unread_count),
                                onclick: expand,
                            }
                        },
                        CardState::PartiallyRead => rsx! {
                            ExpandDivider {
                                icon: EXPAND_ICON,
                                label: match read_count - 2 {
                                    0 => format!("Show comment from {author}"),
                                    1 => format!("Show comment from {author} and 1 earlier reply"),
                                    n => format!("Show comment from {author} and {n} earlier replies"),
                                },
                                onclick: expand,
                            }
                            if let Some(reply) = latest_read_reply {
                                div {
                                    id: is_scroll_anchor.then_some(LATEST_READ_ANCHOR_ID),
                                    TimelineReply { message: reply }
                                }
                            }
                            NewMessagesDivider { unread_count, singular: "reply", plural: "replies" }
                            for reply in unread_replies {
                                TimelineReply { message: reply }
                            }
                        },
                        CardState::Expanded => rsx! {
                            ThreadItem {
                                TimelineMessageView { message: first_message.clone(), review_state }
                            }
                            for reply in read_replies {
                                TimelineReply { message: reply }
                            }
                            if unread_count > 0 && read_count > 0 {
                                NewMessagesDivider { unread_count, singular: "reply", plural: "replies" }
                            }
                            for reply in unread_replies {
                                TimelineReply { message: reply }
                            }
                        },
                        CardState::New => rsx! {
                            UnreadDivider { label: new_label(&kind, replies_count) }
                            ThreadItem {
                                TimelineMessageView { message: first_message.clone(), review_state }
                            }
                            for reply in item.messages[1..].iter().cloned() {
                                TimelineReply { message: reply }
                            }
                        },
                    }
                }
            }
        }
    }
}

fn collapsed_label(
    kind: &TimelineItemKind,
    author: &str,
    replies_count: usize,
    unread_count: usize,
) -> String {
    let replies = match replies_count {
        0 => String::new(),
        1 => " · 1 reply".to_string(),
        n => format!(" · {n} replies"),
    };
    match kind {
        TimelineItemKind::ReviewThread(context) if context.is_resolved => {
            let unread = match unread_count {
                0 => String::new(),
                1 => " · 1 new comment".to_string(),
                n => format!(" · {n} new comments"),
            };
            format!("Resolved thread on {}{unread}", context.path)
        }
        TimelineItemKind::Review(_) => format!("Show review from {author}"),
        _ => format!("Show comment from {author}{replies}"),
    }
}

fn new_label(kind: &TimelineItemKind, replies_count: usize) -> String {
    let noun = match kind {
        TimelineItemKind::Review(_) => "NEW REVIEW",
        _ => "NEW COMMENT",
    };
    match replies_count {
        0 => noun.to_string(),
        1 => format!("{noun} · 1 REPLY"),
        n => format!("{noun} · {n} REPLIES"),
    }
}

#[component]
fn ReviewThreadHeader(context: ReadSignal<ReviewThreadContext>, url: ReadSignal<Url>) -> Element {
    let context = context();
    let lines = match (context.start_line, context.line) {
        (Some(start), Some(end)) if start != end => format!("lines {start}–{end}"),
        (_, Some(line)) => format!("line {line}"),
        _ => String::new(),
    };
    let commented_lines = match (context.start_line, context.line) {
        (Some(start), Some(end)) if end >= start => (end - start + 1) as usize,
        _ => 1,
    };
    let diff_lines = diff_hunk_extract(&context.diff_hunk, commented_lines);

    rsx! {
        div {
            class: "flex flex-col gap-2 mb-2",
            div {
                class: "flex items-center gap-2 flex-wrap text-xs",
                span { class: "icon-[lucide--file-code] size-4 text-ui-base-muted flex-none" }
                a {
                    class: "font-mono truncate",
                    href: "{url}",
                    target: "_blank",
                    rel: "noopener noreferrer",
                    "{context.path}"
                }
                if !lines.is_empty() {
                    span { class: "text-ui-base-muted", "{lines}" }
                }
                if context.is_outdated {
                    UiTag { variant: TagVariant::Warning, "Outdated" }
                }
                if context.is_resolved {
                    UiTag { variant: TagVariant::Success, "Resolved" }
                }
            }
            if !diff_lines.is_empty() {
                DiffHunk { lines: diff_lines }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DiffLineKind {
    Added,
    Removed,
    Context,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub text: String,
}

/// The end of a diff hunk: the commented lines and a few lines of context
/// above them. GitHub's hunk ends with the (last) commented line.
pub fn diff_hunk_extract(diff_hunk: &str, commented_lines: usize) -> Vec<DiffLine> {
    let lines: Vec<&str> = diff_hunk
        .lines()
        .filter(|line| !line.starts_with("@@"))
        .collect();
    let shown = (commented_lines + DIFF_CONTEXT_LINES).min(lines.len());
    lines[lines.len() - shown..]
        .iter()
        .map(|line| {
            let (kind, text) = match line.chars().next() {
                Some('+') => (DiffLineKind::Added, &line[1..]),
                Some('-') => (DiffLineKind::Removed, &line[1..]),
                Some(' ') => (DiffLineKind::Context, &line[1..]),
                _ => (DiffLineKind::Context, *line),
            };
            DiffLine {
                kind,
                text: text.to_string(),
            }
        })
        .collect()
}

#[component]
fn DiffHunk(lines: ReadSignal<Vec<DiffLine>>) -> Element {
    rsx! {
        div {
            class: "font-mono text-[11px] leading-[1.5] rounded-ui-sm border border-ui-border bg-ui-base-200 overflow-x-auto",
            for line in lines() {
                {
                    let (marker, class) = match line.kind {
                        DiffLineKind::Added => ("+", "bg-ui-success-subtle text-ui-success-text"),
                        DiffLineKind::Removed => ("-", "bg-ui-error-subtle text-ui-error-text"),
                        DiffLineKind::Context => (" ", "text-ui-base-content"),
                    };
                    rsx! {
                        div {
                            class: "flex whitespace-pre {class}",
                            span { class: "select-none w-5 flex-none text-center text-ui-base-muted", "{marker}" }
                            span { "{line.text}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TimelineReply(message: ReadSignal<TimelineMessage>) -> Element {
    rsx! {
        ThreadItem {
            ThreadChildren {
                TimelineMessageView { message: message(), review_state: None }
            }
        }
    }
}

#[component]
fn TimelineMessageView(
    message: ReadSignal<TimelineMessage>,
    review_state: ReadSignal<Option<GithubPullRequestReviewState>>,
) -> Element {
    let message = message();
    let (author_name, author_avatar_url) = match message.author {
        Some(actor) => {
            let (name, url) = get_github_actor_name_and_url(actor);
            (name, Some(url))
        }
        None => ("Unknown".to_string(), None),
    };
    let review_tag = review_state().and_then(|state| match state {
        GithubPullRequestReviewState::Approved => Some((TagVariant::Success, "Approved")),
        GithubPullRequestReviewState::ChangesRequested => {
            Some((TagVariant::Error, "Changes requested"))
        }
        GithubPullRequestReviewState::Commented => Some((TagVariant::Info, "Reviewed")),
        GithubPullRequestReviewState::Dismissed => Some((TagVariant::Muted, "Dismissed")),
        GithubPullRequestReviewState::Pending => None,
    });

    rsx! {
        ThreadedMessage {
            author_name,
            author_avatar_url,
            sent_at: Some(message.created_at),
            metadata: review_tag.map(|(variant, label)| rsx! {
                UiTag { variant, "{label}" }
            }),
            body: rsx! {
                div {
                    class: "w-full max-w-full prose prose-sm dark:prose-invert",
                    dangerous_inner_html: sanitize_html(&message.body)
                }
            },
        }
    }
}

fn author_name(author: &Option<GithubActor>) -> String {
    author
        .clone()
        .map(|actor| get_github_actor_name_and_url(actor).0)
        .unwrap_or_else(|| "Unknown".to_string())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use universal_inbox::third_party::integrations::github::{
        GithubIssueComment, GithubMergeStateStatus, GithubMergeableState, GithubPullRequestReview,
        GithubPullRequestReviewComment, GithubPullRequestReviewThread, GithubPullRequestState,
    };
    use wasm_bindgen_test::*;

    use super::*;

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 1, hour, 0, 0).unwrap()
    }

    fn url() -> Url {
        "https://github.com/o/r/pull/1".parse().unwrap()
    }

    fn issue_comment(hour: u32) -> GithubIssueComment {
        GithubIssueComment {
            id: Some(format!("IC_{hour}")),
            url: url(),
            body: format!("<p>{hour}</p>"),
            created_at: at(hour),
            author: None,
        }
    }

    fn review_comment(hour: u32) -> GithubPullRequestReviewComment {
        GithubPullRequestReviewComment {
            id: format!("RC_{hour}"),
            url: url(),
            body: format!("<p>{hour}</p>"),
            created_at: at(hour),
            author: None,
        }
    }

    fn review_thread(id: &str, hours: &[u32], is_resolved: bool) -> GithubPullRequestReviewThread {
        GithubPullRequestReviewThread {
            id: id.to_string(),
            path: "src/lib.rs".to_string(),
            diff_hunk: "@@ -1,2 +1,2 @@\n-a\n+b".to_string(),
            line: Some(1),
            start_line: None,
            is_resolved,
            is_outdated: false,
            comments: hours.iter().map(|hour| review_comment(*hour)).collect(),
        }
    }

    fn review(hour: u32, body: &str) -> GithubPullRequestReview {
        GithubPullRequestReview {
            author: None,
            body: body.to_string(),
            state: GithubPullRequestReviewState::Approved,
            id: Some(format!("PRR_{hour}")),
            url: Some(url()),
            submitted_at: Some(at(hour)),
        }
    }

    fn pull_request(
        comments: Vec<GithubIssueComment>,
        reviews: Vec<GithubPullRequestReview>,
        review_threads: Vec<GithubPullRequestReviewThread>,
    ) -> GithubPullRequest {
        GithubPullRequest {
            id: "PR_1".to_string(),
            number: 1,
            url: url(),
            title: "title".to_string(),
            body: "<p>body</p>".to_string(),
            state: GithubPullRequestState::Open,
            is_draft: false,
            closed_at: None,
            created_at: at(1),
            updated_at: at(9),
            merged_at: None,
            mergeable_state: GithubMergeableState::Mergeable,
            merge_state_status: GithubMergeStateStatus::Clean,
            merged_by: None,
            deletions: 0,
            additions: 0,
            changed_files: 0,
            labels: vec![],
            comments_count: comments.len() as i64,
            comments,
            latest_commit: None,
            base_ref_name: "main".to_string(),
            base_repository: None,
            head_ref_name: "feature".to_string(),
            head_repository: None,
            author: None,
            assignees: vec![],
            review_decision: None,
            reviews,
            review_requests: vec![],
            review_threads,
        }
    }

    fn keys(timeline: &[TimelineItem]) -> Vec<&str> {
        timeline.iter().map(|item| item.key.as_str()).collect()
    }

    #[wasm_bindgen_test]
    fn timeline_merges_comments_reviews_and_threads_chronologically() {
        let pr = pull_request(
            vec![issue_comment(2), issue_comment(6)],
            // Reviews without a body only show in the reviewers section
            vec![review(4, "<p>LGTM</p>"), review(5, "")],
            vec![
                review_thread("PRRT_1", &[3, 7], false),
                review_thread("PRRT_EMPTY", &[], false),
            ],
        );
        assert_eq!(
            keys(&build_timeline(&pr)),
            vec!["IC_2", "PRRT_1", "PRR_4", "IC_6"]
        );
    }

    #[wasm_bindgen_test]
    fn reviews_stored_before_their_date_was_fetched_are_skipped() {
        let pr = pull_request(
            vec![],
            vec![GithubPullRequestReview {
                submitted_at: None,
                ..review(4, "<p>LGTM</p>")
            }],
            vec![],
        );
        assert!(build_timeline(&pr).is_empty());
    }

    #[wasm_bindgen_test]
    fn read_items_are_grouped_and_first_unread_is_the_scroll_anchor() {
        let pr = pull_request(
            vec![issue_comment(2), issue_comment(4), issue_comment(8)],
            vec![],
            // A new reply in a thread started before the read marker
            vec![review_thread("PRRT_1", &[3, 7], false)],
        );
        let timeline = build_timeline(&pr);
        let last_read_at = Some(at(5));
        let grouped: Vec<String> = segments(&timeline, |item| item.has_unread(last_read_at))
            .into_iter()
            .map(|segment| match segment {
                Segment::Comment(item) => item.key,
                Segment::ReadComments(items) => items
                    .into_iter()
                    .map(|item| item.key)
                    .collect::<Vec<_>>()
                    .join("+"),
            })
            .collect();
        assert_eq!(grouped, vec!["IC_2", "PRRT_1", "IC_4", "IC_8"]);
        assert_eq!(
            scroll_anchor_key(&pr, &timeline, last_read_at),
            Some("PRRT_1".to_string())
        );
        // Description unread: the preview stays at the top
        assert_eq!(scroll_anchor_key(&pr, &timeline, None), None);
    }

    #[wasm_bindgen_test]
    fn card_state_follows_read_messages() {
        let pr = pull_request(
            vec![],
            vec![],
            vec![
                review_thread("PRRT_1", &[2, 3, 4], false),
                review_thread("PRRT_2", &[2, 3], true),
            ],
        );
        let timeline = build_timeline(&pr);
        let (thread, resolved) = (&timeline[0], &timeline[1]);
        assert_eq!(card_state(thread, None, false), CardState::New);
        assert_eq!(card_state(thread, Some(at(2)), false), CardState::Expanded);
        assert_eq!(
            card_state(thread, Some(at(3)), false),
            CardState::PartiallyRead
        );
        assert_eq!(card_state(thread, Some(at(9)), false), CardState::Read);
        assert_eq!(card_state(thread, Some(at(9)), true), CardState::Expanded);
        // Resolved threads stay collapsed, even with unread comments
        assert_eq!(card_state(resolved, None, false), CardState::Read);
        assert_eq!(card_state(resolved, Some(at(2)), false), CardState::Read);
        assert_eq!(card_state(resolved, Some(at(2)), true), CardState::Expanded);
        assert_eq!(
            collapsed_label(&resolved.kind, "octocat", 1, 1),
            "Resolved thread on src/lib.rs · 1 new comment"
        );
    }

    #[wasm_bindgen_test]
    fn diff_hunk_extract_keeps_commented_lines_and_context() {
        let hunk = "@@ -1,6 +1,7 @@\n a\n b\n c\n-d\n+e\n+f\n g";
        let text = |lines: Vec<DiffLine>| -> Vec<String> {
            lines
                .into_iter()
                .map(|line| format!("{:?}:{}", line.kind, line.text))
                .collect()
        };
        assert_eq!(
            text(diff_hunk_extract(hunk, 1)),
            vec!["Removed:d", "Added:e", "Added:f", "Context:g"]
        );
        // Multi-line comments show all their lines; short hunks show entirely
        assert_eq!(diff_hunk_extract(hunk, 3).len(), 6);
        assert_eq!(diff_hunk_extract(hunk, 10).len(), 7);
        assert!(diff_hunk_extract("", 1).is_empty());
    }
}
