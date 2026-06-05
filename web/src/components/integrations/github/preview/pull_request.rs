#![allow(non_snake_case)]

use std::{collections::HashMap, default::Default};

use dioxus::prelude::*;
use itertools::Itertools;

use universal_inbox::third_party::integrations::github::{
    GithubActor, GithubBotSummary, GithubCheckConclusionState, GithubCheckRun,
    GithubCheckStatusState, GithubCheckSuite, GithubCheckSuiteApp, GithubCommitChecks,
    GithubIssueComment, GithubLabel, GithubMannequinSummary, GithubMergeableState,
    GithubPullRequest, GithubPullRequestReview, GithubPullRequestReviewState,
    GithubPullRequestState, GithubRepositorySummary, GithubReviewer, GithubTeamSummary,
    GithubUserSummary, GithubWorkflow,
};

use crate::{
    components::{
        Tag, TagList, UserWithAvatar,
        integrations::github::{
            GithubActorDisplay, get_github_actor_name_and_url, icons::GithubPullRequestIcon,
        },
        preview_card_header::PreviewCardHeader,
        thread::{Thread, ThreadItem},
        threaded_message::ThreadedMessage,
        ui::{
            Card, CardVariant, MetadataGrid, MetadataItem, STATUS_ROW_ACTION_CLASS,
            STATUS_ROW_NAME_CLASS, StatusDot, StatusRow, StatusSection, StatusVariant,
        },
    },
    utils::format_elapsed_time,
};

#[component]
pub fn GithubPullRequestPreview(
    github_pull_request: ReadSignal<GithubPullRequest>,
    title: ReadSignal<String>,
    expand_details: ReadSignal<bool>,
) -> Element {
    let pr = github_pull_request();
    let identifier = format!("#{}", pr.number);
    let author = pr.author.clone();
    let created_age = format_elapsed_time(pr.created_at);

    rsx! {
        div {
            class: "flex flex-col w-full h-full",

            PreviewCardHeader {
                brand_icon: rsx! {
                    GithubPullRequestIcon { class: "size-4", github_pull_request: pr.clone() }
                },
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
                        span { "{created_age} ago" }
                    }
                }
            }

            GithubPullRequestDetails { github_pull_request, expand_details }
        }
    }
}

impl From<GithubLabel> for Tag {
    fn from(github_label: GithubLabel) -> Self {
        Tag::Colored {
            name: github_label.name,
            color: github_label.color,
        }
    }
}

#[component]
fn GithubPullRequestDetails(
    github_pull_request: ReadSignal<GithubPullRequest>,
    expand_details: ReadSignal<bool>,
) -> Element {
    let show_base_and_head_repositories = match (
        &github_pull_request().head_repository,
        &github_pull_request().base_repository,
    ) {
        (
            Some(GithubRepositorySummary {
                name_with_owner: head_name_with_owner,
                ..
            }),
            Some(GithubRepositorySummary {
                name_with_owner: base_name_with_owner,
                ..
            }),
        ) => head_name_with_owner != base_name_with_owner,
        _ => false,
    };

    let pr_state_label = match github_pull_request().state {
        GithubPullRequestState::Closed => "Closed",
        GithubPullRequestState::Merged => "Merged",
        GithubPullRequestState::Open => {
            if github_pull_request().is_draft {
                "Draft"
            } else {
                "Opened"
            }
        }
    };

    let (mergeable_state_label, mergeable_state_icon) = match github_pull_request().mergeable_state
    {
        GithubMergeableState::Mergeable => (
            "Pull request is mergeable",
            rsx! { span { class: "icon-[lucide--check-circle-2] size-5 text-success" } },
        ),
        GithubMergeableState::Conflicting => (
            "Pull request is conflicting",
            rsx! { span { class: "icon-[lucide--x-circle] size-5 text-error" } },
        ),
        GithubMergeableState::Unknown => (
            "Unknown pull request mergeable state",
            rsx! { span { class: "icon-[lucide--help-circle] size-5 text-warning" } },
        ),
    };

    rsx! {
        div {
            id: "notification-preview-details",
            class: "flex flex-col gap-2 w-full h-full overflow-y-auto scroll-y-auto p-3",

            Card {
                variant: CardVariant::Default,

                MetadataGrid {
                    MetadataItem {
                        label: "Repository".to_string(),
                        value: rsx! {
                            if let Some(head_repository) = github_pull_request().head_repository {
                                a {
                                    href: "{head_repository.url}",
                                    target: "_blank",
                                    "{head_repository.name_with_owner}"
                                }
                            }
                            a {
                                href: "{github_pull_request().url}",
                                target: "_blank",
                                "#{github_pull_request().number}"
                            }
                        },
                    }

                    MetadataItem {
                        label: "Branch".to_string(),
                        value: rsx! {
                            span { class: "icon-[lucide--git-branch] size-4 flex-none" }
                            if show_base_and_head_repositories {
                                if let Some(head_repository) = github_pull_request().head_repository {
                                    a {
                                        href: "{head_repository.url}",
                                        target: "_blank",
                                        "{head_repository.name_with_owner}:"
                                    }
                                }
                            }
                            span {
                                style: "font-family: var(--font-mono); font-size: 11px; padding: 0 6px; border-radius: var(--ui-radius-xs); background: var(--ui-base-200); color: var(--ui-primary);",
                                "{github_pull_request().head_ref_name}"
                            }
                            span {
                                style: "color: var(--ui-base-muted);",
                                class: "icon-[lucide--arrow-right] size-3.5"
                            }
                            if show_base_and_head_repositories {
                                if let Some(base_repository) = github_pull_request().base_repository {
                                    a {
                                        href: "{base_repository.url}",
                                        target: "_blank",
                                        "{base_repository.name_with_owner}:"
                                    }
                                }
                            }
                            span {
                                style: "font-family: var(--font-mono); font-size: 11px; padding: 0 6px; border-radius: var(--ui-radius-xs); background: var(--ui-base-200); color: var(--ui-primary);",
                                "{github_pull_request().base_ref_name}"
                            }
                        },
                    }

                    MetadataItem {
                        label: "Changes".to_string(),
                        value: rsx! {
                            span { style: "color: var(--ui-success); font-weight: 600;", "+{github_pull_request().additions}" }
                            span { style: "margin: 0 6px; color: var(--ui-base-muted);", "·" }
                            span { style: "color: var(--ui-error); font-weight: 600;", "-{github_pull_request().deletions}" }
                            span { style: "margin: 0 6px; color: var(--ui-base-muted);", "in" }
                            span { style: "color: var(--ui-primary); font-weight: 600;", "{github_pull_request().changed_files}" }
                            span { style: "color: var(--ui-base-muted);", " files" }
                        },
                    }

                    MetadataItem {
                        label: "Status".to_string(),
                        value: rsx! {
                            GithubPullRequestIcon { class: "h-4 w-4", github_pull_request: github_pull_request() }
                            span { "{pr_state_label}" }
                        },
                    }

                    if github_pull_request().state == GithubPullRequestState::Open {
                        MetadataItem {
                            label: "Mergeable".to_string(),
                            value: rsx! {
                                { mergeable_state_icon }
                                span { "{mergeable_state_label}" }
                            },
                        }
                    }

                    if !github_pull_request().assignees.is_empty() {
                        MetadataItem {
                            label: "Assigned to".to_string(),
                            value: rsx! {
                                for assignee in github_pull_request().assignees {
                                    GithubActorDisplay { actor: assignee, display_name: true }
                                }
                            },
                        }
                    }

                    if let Some(merged_by) = github_pull_request().merged_by {
                        MetadataItem {
                            label: "Merged by".to_string(),
                            value: rsx! {
                                GithubActorDisplay { actor: merged_by, display_name: true }
                            },
                        }
                    }
                }

                TagList {
                    tags: github_pull_request()
                        .labels
                        .iter()
                        .map(|label| label.clone().into())
                        .collect()
                }

                ReviewsSection { github_pull_request, expand_details }

                ChecksSection {
                    latest_commit: github_pull_request().latest_commit,
                    expand_details,
                }
            }

            if !github_pull_request().body.is_empty() {
                Card {
                    variant: CardVariant::Default,
                    p {
                        class: "w-full max-w-full prose prose-sm dark:prose-invert",
                        dangerous_inner_html: "{github_pull_request().body}"
                    }
                }
            }

            GithubCommentList { comments: github_pull_request().comments }
        }
    }
}

#[component]
fn ChecksSection(
    latest_commit: ReadSignal<GithubCommitChecks>,
    expand_details: ReadSignal<bool>,
) -> Element {
    let progress_memo =
        use_memo(move || compute_pull_request_checks_progress(&latest_commit().check_suites));
    let Some(progress) = progress_memo() else {
        return rsx! {};
    };

    let dot_variant = checks_dot_variant(&progress);
    let summary = format_checks_summary(&progress);

    rsx! {
        div {
            class: "border-t border-ui-border-light mt-2.5 -mx-3 px-3",

            StatusSection {
                dot: rsx! { StatusDot { variant: dot_variant } },
                label: "Checks".to_string(),
                summary,
                expand: expand_details,

                ChecksSectionList { latest_commit }
            }
        }
    }
}

#[component]
fn ChecksSectionList(latest_commit: ReadSignal<GithubCommitChecks>) -> Element {
    let Some(check_suites) = &latest_commit().check_suites else {
        return rsx! {};
    };

    rsx! {
        for check_suite in check_suites {
            if check_suite.status != GithubCheckStatusState::Queued {
                for check_run in check_suite.check_runs.iter() {
                    GithubCheckRunRow {
                        check_run: check_run.clone(),
                        workflow: check_suite.workflow.clone(),
                        app: check_suite.app.clone(),
                    }
                }
            }
        }
    }
}

#[component]
fn GithubCheckRunRow(
    check_run: ReadSignal<GithubCheckRun>,
    workflow: Option<GithubWorkflow>,
    app: Option<GithubCheckSuiteApp>,
) -> Element {
    let run = check_run();
    let row_variant = match run.status {
        GithubCheckStatusState::Completed => match run.conclusion {
            Some(GithubCheckConclusionState::Success) | None => StatusVariant::Default,
            Some(_) => StatusVariant::Error,
        },
        GithubCheckStatusState::InProgress => StatusVariant::Warning,
        _ => StatusVariant::Default,
    };

    let status_icon = match run.status {
        GithubCheckStatusState::Completed => match run.conclusion {
            Some(GithubCheckConclusionState::Success) => {
                rsx! { span { class: "icon-[lucide--check-circle-2] size-4 text-success flex-none" } }
            }
            Some(GithubCheckConclusionState::Failure) => {
                rsx! { span { class: "icon-[lucide--x-circle] size-4 text-error flex-none" } }
            }
            _ => {
                rsx! { span { class: "icon-[lucide--help-circle] size-4 text-warning flex-none" } }
            }
        },
        GithubCheckStatusState::InProgress => {
            rsx! { span { class: "size-4 loading loading-spinner text-warning flex-none" } }
        }
        GithubCheckStatusState::Pending | GithubCheckStatusState::Waiting => {
            rsx! { span { class: "icon-[lucide--pause-circle] size-4 text-base-muted flex-none" } }
        }
        GithubCheckStatusState::Queued => {
            rsx! { span { class: "icon-[lucide--skip-forward] size-4 text-base-muted flex-none" } }
        }
        GithubCheckStatusState::Requested => {
            rsx! { span { class: "icon-[lucide--help-circle] size-4 text-base-muted flex-none" } }
        }
    };

    let display_name = match (workflow.as_ref(), app.as_ref()) {
        (Some(w), _) => format!("{} / {}", w.name, run.name),
        (None, Some(a)) => format!("{} / {}", a.name, run.name),
        _ => run.name.clone(),
    };
    let name_url = workflow
        .as_ref()
        .map(|w| w.url.to_string())
        .or_else(|| run.url.as_ref().map(|u| u.to_string()));
    let details_url = run.url.as_ref().map(|u| u.to_string());

    rsx! {
        StatusRow {
            variant: row_variant,

            div {
                class: "{STATUS_ROW_NAME_CLASS}",
                { status_icon }
                if let Some(url) = name_url {
                    a {
                        href: "{url}",
                        target: "_blank",
                        rel: "noopener noreferrer",
                        title: "{display_name}",
                        "{display_name}"
                    }
                } else {
                    span { title: "{display_name}", "{display_name}" }
                }
            }

            if let Some(url) = details_url {
                a {
                    class: "{STATUS_ROW_ACTION_CLASS}",
                    href: "{url}",
                    target: "_blank",
                    rel: "noopener noreferrer",
                    "Details"
                }
            }
        }
    }
}

fn checks_dot_variant(progress: &GithubChecksProgress) -> StatusVariant {
    let successful = progress.completed_checks_count - progress.failed_checks_count;
    let running = progress.checks_count - progress.completed_checks_count;
    if running > 0 {
        StatusVariant::Warning
    } else if progress.failed_checks_count > 0 {
        StatusVariant::Error
    } else if successful > 0 {
        StatusVariant::Success
    } else {
        StatusVariant::Default
    }
}

fn format_checks_summary(progress: &GithubChecksProgress) -> String {
    let successful = progress
        .completed_checks_count
        .saturating_sub(progress.failed_checks_count);
    let running = progress
        .checks_count
        .saturating_sub(progress.completed_checks_count);
    let failing = progress.failed_checks_count;

    let mut parts: Vec<String> = Vec::new();
    if successful > 0 {
        parts.push(format!("{successful} successful"));
    }
    if running > 0 {
        parts.push(format!("{running} running"));
    }
    if failing > 0 {
        parts.push(format!("{failing} failing"));
    }
    if parts.is_empty() {
        format!("{} total", progress.checks_count)
    } else {
        parts.join(", ")
    }
}

#[derive(PartialEq, Eq, Debug, Clone, Default)]
struct GithubChecksProgress {
    checks_count: usize,
    completed_checks_count: usize,
    failed_checks_count: usize,
}

fn compute_pull_request_checks_progress(
    check_suites: &Option<Vec<GithubCheckSuite>>,
) -> Option<GithubChecksProgress> {
    check_suites.as_ref().and_then(|check_suites| {
        let mut progress = GithubChecksProgress {
            ..Default::default()
        };
        for check_suite in check_suites {
            if check_suite.status != GithubCheckStatusState::Queued {
                for check_run in check_suite.check_runs.iter() {
                    progress.checks_count += 1;
                    if check_run.status == GithubCheckStatusState::Completed {
                        progress.completed_checks_count += 1;
                        if let Some(conclusion) = &check_run.conclusion
                            && *conclusion != GithubCheckConclusionState::Success
                        {
                            progress.failed_checks_count += 1;
                        }
                    }
                }
            }
        }

        if progress.checks_count == 0 {
            None
        } else {
            Some(progress)
        }
    })
}

#[component]
fn ReviewsSection(
    github_pull_request: ReadSignal<GithubPullRequest>,
    expand_details: ReadSignal<bool>,
) -> Element {
    let reviews = compute_pull_request_reviews(
        github_pull_request().reviews.as_ref(),
        github_pull_request().review_requests.as_ref(),
    );
    if reviews.is_empty() {
        return rsx! {};
    }

    let dot_variant = reviews_dot_variant(&reviews);
    let summary = format_reviews_summary(&reviews);

    rsx! {
        div {
            class: "border-t border-ui-border-light mt-2.5 -mx-3 px-3",

            StatusSection {
                dot: rsx! { StatusDot { variant: dot_variant } },
                label: "Reviewers".to_string(),
                summary,
                expand: expand_details,

                for review in reviews {
                    GithubReviewRow { review }
                }
            }
        }
    }
}

#[component]
fn GithubReviewRow(review: GithubReview) -> Element {
    let (reviewer, review_body, review_status_icon, row_variant) = match review {
        GithubReview::Requested { reviewer } => (
            reviewer,
            None,
            rsx! { span { class: "icon-[lucide--clock] size-4 text-info flex-none" } },
            StatusVariant::Default,
        ),
        GithubReview::Completed {
            reviewer,
            state,
            body,
        } => {
            let body = (!body.is_empty()).then_some(body);
            let (icon, variant) = match state {
                GithubPullRequestReviewState::Approved => (
                    rsx! { span { class: "icon-[lucide--check-circle-2] size-4 text-success flex-none" } },
                    StatusVariant::Default,
                ),
                GithubPullRequestReviewState::ChangesRequested => (
                    rsx! { span { class: "icon-[lucide--x-circle] size-4 text-error flex-none" } },
                    StatusVariant::Error,
                ),
                GithubPullRequestReviewState::Commented => (
                    rsx! { span { class: "icon-[lucide--message-square] size-4 text-info flex-none" } },
                    StatusVariant::Default,
                ),
                _ => (
                    rsx! { span { class: "icon-[lucide--help-circle] size-4 text-neutral flex-none" } },
                    StatusVariant::Default,
                ),
            };
            (reviewer, body, icon, variant)
        }
    };
    let (reviewer_display_name, reviewer_avatar_url) = match reviewer {
        GithubReviewer::User(GithubUserSummary {
            name,
            avatar_url,
            login,
        }) => (
            name.clone().unwrap_or(login.clone()),
            Some(avatar_url.clone()),
        ),
        GithubReviewer::Bot(GithubBotSummary {
            login, avatar_url, ..
        }) => (login.clone(), Some(avatar_url.clone())),
        GithubReviewer::Team(GithubTeamSummary {
            name, avatar_url, ..
        }) => (name.clone(), avatar_url.clone()),
        GithubReviewer::Mannequin(GithubMannequinSummary {
            login, avatar_url, ..
        }) => (login.clone(), Some(avatar_url.clone())),
    };

    if let Some(review_body) = review_body {
        // Reviewer header + review comment, both shown inline once the
        // Reviewers section is open — single level of expansion, no nested
        // collapse. The column-flex shell with tighter padding isn't a fit for
        // the `StatusRow` shape, so we reuse the variant tint + row radius/font
        // to stay in family.
        let tint = match row_variant {
            StatusVariant::Error => "bg-ui-error-subtle",
            StatusVariant::Warning => "bg-ui-warning-subtle",
            _ => "",
        };
        let class = format!(
            "flex flex-col items-stretch gap-2.5 px-1 py-0.5 rounded-ui-sm text-[12.5px] {tint}"
        );
        rsx! {
            div {
                class: "{class}",
                div {
                    class: "flex gap-2 items-center w-full p-2",
                    { review_status_icon }
                    UserWithAvatar {
                        user_name: reviewer_display_name.clone(),
                        avatar_url: reviewer_avatar_url,
                        display_name: true,
                    },
                }

                div {
                    class: "bg-neutral text-neutral-content p-2 my-1 rounded-sm",
                    dangerous_inner_html: "{review_body}"
                }
            }
        }
    } else {
        rsx! {
            StatusRow {
                variant: row_variant,
                div {
                    class: "{STATUS_ROW_NAME_CLASS}",
                    { review_status_icon }
                    UserWithAvatar {
                        user_name: reviewer_display_name.clone(),
                        avatar_url: reviewer_avatar_url,
                        display_name: true,
                    },
                }
            }
        }
    }
}

fn reviews_dot_variant(reviews: &[GithubReview]) -> StatusVariant {
    let mut has_changes_requested = false;
    let mut has_pending = false;
    let mut has_approved = false;
    for review in reviews {
        match review {
            GithubReview::Requested { .. } => has_pending = true,
            GithubReview::Completed { state, .. } => match state {
                GithubPullRequestReviewState::ChangesRequested => has_changes_requested = true,
                GithubPullRequestReviewState::Approved => has_approved = true,
                _ => {}
            },
        }
    }
    if has_changes_requested {
        StatusVariant::Error
    } else if has_pending {
        StatusVariant::Warning
    } else if has_approved {
        StatusVariant::Success
    } else {
        StatusVariant::Default
    }
}

fn format_reviews_summary(reviews: &[GithubReview]) -> String {
    let mut approved = 0usize;
    let mut changes_requested = 0usize;
    let mut commented = 0usize;
    let mut pending = 0usize;
    for review in reviews {
        match review {
            GithubReview::Requested { .. } => pending += 1,
            GithubReview::Completed { state, .. } => match state {
                GithubPullRequestReviewState::Approved => approved += 1,
                GithubPullRequestReviewState::ChangesRequested => changes_requested += 1,
                GithubPullRequestReviewState::Commented => commented += 1,
                _ => {}
            },
        }
    }

    let mut parts: Vec<String> = Vec::new();
    if approved > 0 {
        parts.push(format!("{approved} approved"));
    }
    if changes_requested > 0 {
        parts.push(format!("{changes_requested} changes requested"));
    }
    if commented > 0 {
        parts.push(format!("{commented} commented"));
    }
    if pending > 0 {
        parts.push(format!("{pending} pending"));
    }
    if parts.is_empty() {
        format!("{} total", reviews.len())
    } else {
        parts.join(", ")
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum GithubReview {
    Requested {
        reviewer: GithubReviewer,
    },
    Completed {
        reviewer: GithubReviewer,
        body: String,
        state: GithubPullRequestReviewState,
    },
}

pub fn compute_pull_request_reviews(
    reviews: &[GithubPullRequestReview],
    review_requests: &[GithubReviewer],
) -> Vec<GithubReview> {
    let mut result = HashMap::new();
    for review_request in review_requests {
        let request_key = match review_request {
            GithubReviewer::User(GithubUserSummary { login, .. }) => login.clone(),
            GithubReviewer::Bot(GithubBotSummary { login, .. }) => login.clone(),
            GithubReviewer::Team(GithubTeamSummary { name, .. }) => name.clone(),
            GithubReviewer::Mannequin(GithubMannequinSummary { login, .. }) => login.clone(),
        };
        result.insert(
            request_key,
            GithubReview::Requested {
                reviewer: review_request.clone(),
            },
        );
    }

    for review in reviews {
        if let Some(author) = &review.author {
            let review_key = match author {
                GithubActor::User(GithubUserSummary { login, .. }) => login.clone(),
                GithubActor::Bot(GithubBotSummary { login, .. }) => login.clone(),
            };
            let author = match author {
                GithubActor::User(user) => GithubReviewer::User(user.clone()),
                GithubActor::Bot(bot) => GithubReviewer::Bot(bot.clone()),
            };
            result.insert(
                review_key,
                GithubReview::Completed {
                    reviewer: author,
                    body: review.body.clone(),
                    state: review.state,
                },
            );
        }
    }

    result
        .into_iter()
        .sorted_by(|(k1, _), (k2, _)| Ord::cmp(&k1, &k2))
        .map(|(_, v)| v)
        .collect()
}

#[component]
pub fn GithubCommentList(comments: ReadSignal<Vec<GithubIssueComment>>) -> Element {
    let comments_v = comments();
    if comments_v.is_empty() {
        return rsx! {};
    }
    rsx! {
        Card {
            variant: CardVariant::Default,
            Thread {
                for comment in comments_v {
                    GithubCommentRow { comment }
                }
            }
        }
    }
}

#[component]
fn GithubCommentRow(comment: ReadSignal<GithubIssueComment>) -> Element {
    let c = comment();
    let body_html = c.body.clone();
    let (author_name, avatar_url, subtitle) = match c.author {
        Some(actor) => {
            let login = github_actor_login(&actor);
            let (name, url) = get_github_actor_name_and_url(actor);
            let sub = match login {
                Some(l) if l != name => Some(format!("@{l}")),
                _ => None,
            };
            (name, Some(url), sub)
        }
        None => ("Unknown".to_string(), None, None),
    };

    rsx! {
        ThreadItem {
            ThreadedMessage {
                author_name,
                author_avatar_url: avatar_url,
                author_subtitle: subtitle,
                sent_at: Some(c.created_at),
                body: rsx! {
                    span { class: "prose prose-sm", dangerous_inner_html: "{body_html}" }
                },
            }
        }
    }
}

fn github_actor_login(actor: &GithubActor) -> Option<String> {
    match actor {
        GithubActor::User(u) => Some(u.login.clone()),
        GithubActor::Bot(b) => Some(b.login.clone()),
    }
}

#[cfg(test)]
mod tests {
    mod compute_pull_request_progress {
        use super::super::*;
        use pretty_assertions::assert_eq;
        use wasm_bindgen_test::*;

        #[wasm_bindgen_test]
        fn test_no_progress_without_check_suites() {
            assert!(compute_pull_request_checks_progress(&Some(vec![])).is_none());
            assert!(compute_pull_request_checks_progress(&None).is_none());
        }

        #[wasm_bindgen_test]
        fn test_progress_for_uncomplete_check_suites() {
            assert_eq!(
                compute_pull_request_checks_progress(&Some(vec![
                    GithubCheckSuite {
                        status: GithubCheckStatusState::Queued, // ignored
                        ..Default::default()
                    },
                    GithubCheckSuite {
                        status: GithubCheckStatusState::InProgress,
                        check_runs: vec![
                            GithubCheckRun {
                                status: GithubCheckStatusState::InProgress, // ignored
                                ..Default::default()
                            },
                            GithubCheckRun {
                                status: GithubCheckStatusState::Pending, // ignored
                                ..Default::default()
                            },
                            GithubCheckRun {
                                status: GithubCheckStatusState::Queued, // ignored
                                ..Default::default()
                            },
                            GithubCheckRun {
                                status: GithubCheckStatusState::Requested, // ignored
                                ..Default::default()
                            },
                            GithubCheckRun {
                                status: GithubCheckStatusState::Waiting, // ignored
                                ..Default::default()
                            },
                        ],
                        ..Default::default()
                    },
                ])),
                Some(GithubChecksProgress {
                    checks_count: 5,
                    completed_checks_count: 0,
                    failed_checks_count: 0,
                })
            );
        }

        #[wasm_bindgen_test]
        fn test_progress_for_complete_check_suites() {
            assert_eq!(
                compute_pull_request_checks_progress(&Some(vec![GithubCheckSuite {
                    status: GithubCheckStatusState::Completed,
                    check_runs: vec![
                        GithubCheckRun {
                            status: GithubCheckStatusState::Completed,
                            conclusion: Some(GithubCheckConclusionState::Success),
                            ..Default::default()
                        },
                        GithubCheckRun {
                            status: GithubCheckStatusState::Completed,
                            conclusion: Some(GithubCheckConclusionState::Failure),
                            ..Default::default()
                        },
                        GithubCheckRun {
                            status: GithubCheckStatusState::Completed,
                            conclusion: Some(GithubCheckConclusionState::Cancelled),
                            ..Default::default()
                        },
                        GithubCheckRun {
                            status: GithubCheckStatusState::InProgress, // ignored
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },])),
                Some(GithubChecksProgress {
                    checks_count: 4,
                    completed_checks_count: 3,
                    failed_checks_count: 2,
                })
            );
        }
    }

    mod compute_pull_request_reviews {
        use super::super::*;
        use pretty_assertions::assert_eq;
        use wasm_bindgen_test::*;

        #[wasm_bindgen_test]
        fn test_no_reviews_no_requests() {
            assert!(compute_pull_request_reviews(&[], &[]).is_empty());
        }

        #[wasm_bindgen_test]
        fn test_with_reviews_and_requests_no_intersection() {
            let requested_reviewer = GithubReviewer::User(GithubUserSummary {
                login: "user1".to_string(),
                avatar_url: "https://example.com".parse().unwrap(),
                name: None,
            });
            let reviewer = GithubReviewer::User(GithubUserSummary {
                login: "user2".to_string(),
                avatar_url: "https://example.com".parse().unwrap(),
                name: None,
            });

            assert_eq!(
                compute_pull_request_reviews(
                    &[
                        GithubPullRequestReview {
                            author: Some(GithubActor::User(GithubUserSummary {
                                login: "user2".to_string(),
                                avatar_url: "https://example.com".parse().unwrap(),
                                name: None,
                            })),
                            body: "my review".to_string(),
                            state: GithubPullRequestReviewState::Approved,
                        },
                        // ignored
                        GithubPullRequestReview {
                            author: None,
                            body: "".to_string(),
                            state: GithubPullRequestReviewState::Approved,
                        }
                    ],
                    std::slice::from_ref(&requested_reviewer)
                ),
                vec![
                    GithubReview::Requested {
                        reviewer: requested_reviewer,
                    },
                    GithubReview::Completed {
                        reviewer,
                        body: "my review".to_string(),
                        state: GithubPullRequestReviewState::Approved,
                    },
                ]
            );
        }

        #[wasm_bindgen_test]
        fn test_with_reviews_and_requests_overlapping() {
            let requested_reviewer = GithubReviewer::User(GithubUserSummary {
                login: "user1".to_string(),
                avatar_url: "https://example.com".parse().unwrap(),
                name: None,
            });

            assert_eq!(
                compute_pull_request_reviews(
                    &[
                        // Review from requested_reviewer
                        GithubPullRequestReview {
                            author: Some(GithubActor::User(GithubUserSummary {
                                login: "user1".to_string(),
                                avatar_url: "https://example.com".parse().unwrap(),
                                name: None,
                            })),
                            body: "my review".to_string(),
                            state: GithubPullRequestReviewState::Approved,
                        }
                    ],
                    std::slice::from_ref(&requested_reviewer)
                ),
                vec![GithubReview::Completed {
                    reviewer: requested_reviewer,
                    body: "my review".to_string(),
                    state: GithubPullRequestReviewState::Approved,
                },]
            );
        }
    }
}
