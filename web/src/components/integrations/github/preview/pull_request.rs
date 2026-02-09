#![allow(non_snake_case)]

use std::{collections::HashMap, default::Default};

use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::bs_icons::{
        BsArrowUpRightSquare, BsChatTextFill, BsCheckCircleFill, BsClock, BsPauseCircleFill,
        BsQuestionCircleFill, BsSkipForwardCircleFill, BsXCircleFill,
    },
};
use itertools::Itertools;

use universal_inbox::third_party::integrations::github::{
    GithubActor, GithubBotSummary, GithubCheckConclusionState, GithubCheckRun,
    GithubCheckStatusState, GithubCheckSuite, GithubCheckSuiteApp, GithubCommitChecks,
    GithubIssueComment, GithubLabel, GithubMannequinSummary, GithubMergeableState,
    GithubPullRequest, GithubPullRequestReview, GithubPullRequestReviewDecision,
    GithubPullRequestReviewState, GithubPullRequestState, GithubRepositorySummary, GithubReviewer,
    GithubTeamSummary, GithubUserSummary, GithubWorkflow,
};
use uuid::Uuid;

use crate::components::{
    CardWithHeaders, CollapseCardWithIcon, MessageHeader, SmallCard, Tag, TagsInCard,
    UserWithAvatar,
    flyonui::collapse::Collapse,
    integrations::github::{
        GithubActorDisplay, get_github_actor_name_and_url, icons::GithubPullRequestIcon,
    },
};

#[component]
pub fn GithubPullRequestPreview(
    github_pull_request: ReadSignal<GithubPullRequest>,
    expand_details: ReadSignal<bool>,
) -> Element {
    rsx! {
        div {
            class: "flex flex-col w-full gap-2 h-full",

            h3 {
                class: "flex items-center gap-2 text-base",

                GithubPullRequestIcon { class: "h-5 w-5", github_pull_request: github_pull_request() }
                a {
                    href: "{github_pull_request().url}",
                    target: "_blank",
                    dangerous_inner_html: "{github_pull_request().title}"
                }
                a {
                    class: "flex-none",
                    href: "{github_pull_request().url}",
                    target: "_blank",
                    Icon { class: "h-5 w-5 min-w-5 text-base-content/50 p-1", icon: BsArrowUpRightSquare }
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
            rsx! { Icon { class: "h-5 w-5 text-success", icon: BsCheckCircleFill } },
        ),
        GithubMergeableState::Conflicting => (
            "Pull request is conflicting",
            rsx! { Icon { class: "h-5 w-5 text-error", icon: BsXCircleFill } },
        ),
        GithubMergeableState::Unknown => (
            "Unknown pull request mergeable state",
            rsx! { Icon { class: "h-5 w-5 text-warning", icon: BsQuestionCircleFill } },
        ),
    };

    rsx! {
        div {
            id: "notification-preview-details",
            class: "flex flex-col gap-2 w-full h-full overflow-y-auto scroll-y-auto",

            div {
                class: "flex gap-2",

                if let Some(head_repository) = github_pull_request().head_repository {
                    a {
                        class: "text-xs text-base-content/50",
                        href: "{head_repository.url}",
                        target: "_blank",
                        "{head_repository.name_with_owner}"
                    }
                }

                a {
                    class: "text-xs text-base-content/50",
                    href: "{github_pull_request().url}",
                    target: "_blank",
                    "#{github_pull_request().number}"
                }
            }

            div {
                class: "flex text-base-content/50 gap-1 text-xs",

                "Created at ",
                span { class: "text-primary", "{github_pull_request().created_at}" }
            }

            div {
                class: "flex flex-wrap items-center text-base-content/50 gap-1 text-xs",

                "From"
                if show_base_and_head_repositories {
                    if let Some(head_repository) = github_pull_request().head_repository {
                        a {
                            href: "{head_repository.url}",
                            target: "_blank",
                            "{head_repository.name_with_owner}:"
                        }
                    }
                }
                span { class: "text-primary", "{github_pull_request().head_ref_name}" }

                "into"

                if show_base_and_head_repositories {
                    if let Some(base_repository) = github_pull_request().base_repository {
                        a {
                            href: "{base_repository.url}",
                            target: "_blank",
                            "{base_repository.name_with_owner}:"
                        }
                    }
                }
                span { class: "text-primary", "{github_pull_request().base_ref_name}" }
            }

            div {
                class: "flex flex-wrap items-center text-base-content/50 gap-1 text-xs",
                span { class: "text-red-500", "-{github_pull_request().deletions}" }
                span { "/" }
                span { class: "text-green-500", "+{github_pull_request().additions}" }
                span { "in" }
                span { class: "text-primary", "{github_pull_request().changed_files}" }
                span { "files" }
            }

            TagsInCard {
                tags: github_pull_request()
                    .labels
                    .iter()
                    .map(|label| label.clone().into())
                    .collect()
            }

            if let Some(actor) = github_pull_request().author {
                SmallCard {
                    span { class: "text-base-content/50", "Opened by" }
                    GithubActorDisplay { actor: actor, display_name: true }
                }
            }

            if !github_pull_request().assignees.is_empty() {
                SmallCard {
                    span { class: "text-base-content/50", "Assigned to" }
                    div {
                        class: "flex flex-col",
                        for assignee in github_pull_request().assignees {
                            GithubActorDisplay { actor: assignee, display_name: true }
                        }
                    }
                }
            }

            if let Some(merged_by) = github_pull_request().merged_by {
                SmallCard {
                    span { class: "text-base-content/50", "Merged by" }
                    GithubActorDisplay { actor: merged_by, display_name: true }
                }
            }

            SmallCard {
                GithubPullRequestIcon { class: "h-5 w-5", github_pull_request: github_pull_request() }
                span { "{pr_state_label}" }
            }

            if github_pull_request().state == GithubPullRequestState::Open {
                SmallCard { { mergeable_state_icon }, span { "{mergeable_state_label}" } }
            }

            ReviewsGithubPullRequest { github_pull_request, expand_details }

            ChecksGithubPullRequest {
                latest_commit: github_pull_request().latest_commit,
                with_details: true,
                expand_details
            }

            p {
                class: "w-full prose prose-sm dark:prose-invert",
                dangerous_inner_html: "{github_pull_request().body}"
            }

            GithubCommentList { comments: github_pull_request().comments }
        }
    }
}

#[component]
pub fn ChecksGithubPullRequest(
    latest_commit: ReadSignal<GithubCommitChecks>,
    with_details: Option<bool>,
    icon_size: Option<String>,
    expand_details: ReadSignal<bool>,
) -> Element {
    let with_details = with_details.unwrap_or_default();
    let checks_progress =
        use_memo(move || compute_pull_request_checks_progress(&latest_commit().check_suites));
    let icon_size = icon_size.unwrap_or_else(|| "h-5 w-5".to_string());

    let checks_state = checks_progress.as_ref().map(|checks_progress| {
        match checks_progress.status() {
            GithubCheckStatusState::Pending => (
                "Checks not started yet",
                rsx! { Icon { class: "{icon_size} text-success", icon: BsPauseCircleFill } },
            ),
            GithubCheckStatusState::InProgress => (
                "Checks are in progress",
                rsx! { span { class: "{icon_size} loading loading-spinner text-primary" } },
            ),
            GithubCheckStatusState::Completed => match checks_progress.conclusion() {
                GithubCheckConclusionState::Success => (
                    "All checks passed",
                    rsx! { Icon { class: "{icon_size} text-success", icon: BsCheckCircleFill } },
                ),
                GithubCheckConclusionState::Failure => (
                    "Some checks failed",
                    rsx! { Icon { class: "{icon_size} text-error", icon: BsXCircleFill } },
                ),
                _ => (
                    "Unknown checks status",
                    rsx! { Icon { class: "{icon_size} text-warning", icon: BsQuestionCircleFill } },
                ),
            },
            _ => (
                "Unknown checks status",
                rsx! { Icon { class: "{icon_size} text-warning", icon: BsQuestionCircleFill } },
            ),
        }
    });

    let Some(checks_state) = checks_state else {
        return rsx! {};
    };

    if with_details {
        return rsx! {
            CollapseCardWithIcon {
                id: "checks-github-pull-request",
                title: "{checks_state.0}",
                icon: checks_state.1,
                opened: expand_details(),
                ChecksGithubPullRequestDetails { latest_commit: latest_commit }
            }
        };
    } else {
        checks_state.1
    }
}

#[component]
fn ChecksGithubPullRequestDetails(latest_commit: ReadSignal<GithubCommitChecks>) -> Element {
    let Some(check_suites) = &latest_commit().check_suites else {
        return rsx! {};
    };

    rsx! {
        table {
            class: "table table-auto table-xs w-full",
            tbody {
                for check_suite in check_suites {
                    if check_suite.status != GithubCheckStatusState::Queued {
                        for check_run in check_suite.check_runs.iter() {
                            GithubCheckRunLine {
                                check_run: check_run.clone(),
                                workflow: check_suite.workflow.clone(),
                                app: check_suite.app.clone(),
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn GithubCheckRunLine(
    check_run: ReadSignal<GithubCheckRun>,
    workflow: Option<GithubWorkflow>,
    app: Option<GithubCheckSuiteApp>,
) -> Element {
    let check_run_status_icon = match check_run().status {
        GithubCheckStatusState::Completed => match check_run().conclusion {
            Some(GithubCheckConclusionState::Success) => {
                rsx! { Icon { class: "h-5 w-5 text-success", icon: BsCheckCircleFill } }
            }
            Some(GithubCheckConclusionState::Failure) => {
                rsx! { Icon { class: "h-5 w-5 text-error", icon: BsXCircleFill } }
            }
            _ => rsx! { Icon { class: "h-5 w-5 text-warning", icon: BsQuestionCircleFill } },
        },
        GithubCheckStatusState::InProgress => {
            rsx! { span { class: "h-5 w-5 loading loading-spinner text-primary" } }
        }
        GithubCheckStatusState::Pending => {
            rsx! { Icon { class: "h-5 w-5 text-base-content", icon: BsPauseCircleFill } }
        }
        GithubCheckStatusState::Queued => {
            rsx! { Icon { class: "h-5 w-5 text-base-content", icon: BsSkipForwardCircleFill } }
        }
        GithubCheckStatusState::Requested => {
            rsx! { Icon { class: "h-5 w-5 text-base-content", icon: BsQuestionCircleFill } }
        }
        GithubCheckStatusState::Waiting => {
            rsx! { Icon { class: "h-5 w-5 text-base-content", icon: BsPauseCircleFill } }
        }
    };

    rsx! {
        tr {
            td {
                div {
                    class: "flex items-center gap-1",
                    { check_run_status_icon }

                    if let Some(app) = app {
                        a {
                            href: "{app.url}",
                            target: "_blank",

                            if let Some(logo_url) = app.logo_url {
                                img {
                                    class: "h-5 w-5 rounded-full",
                                    alt: "{app.name}",
                                    title: "{app.name}",
                                    src: "{logo_url}"
                                }
                            } else {
                                Icon { class: "h-5 w-5 rounded-full", icon: BsQuestionCircleFill }
                            }
                        }
                    } else {
                        Icon { class: "h-5 w-5", icon: BsQuestionCircleFill }
                    }

                    if let Some(workflow) = workflow {
                        a {
                            class: "text-primary",
                            href: "{workflow.url}",
                            target: "_blank",
                            "{workflow.name}"
                        }
                    } else if let Some(check_run_url) = check_run().url {
                        a {
                            class: "text-primary",
                            href: "{check_run_url}",
                            target: "_blank",
                            "{check_run().name}"
                        }
                    } else {
                        span { class: "text-primary", "{check_run().name}" }
                    }
                }
            }
        }
    }
}

#[derive(PartialEq, Eq, Debug, Clone, Default)]
struct GithubChecksProgress {
    checks_count: usize,
    completed_checks_count: usize,
    failed_checks_count: usize,
}

impl GithubChecksProgress {
    pub fn status(&self) -> GithubCheckStatusState {
        if self.completed_checks_count == 0 {
            GithubCheckStatusState::Pending
        } else if self.completed_checks_count == self.checks_count {
            GithubCheckStatusState::Completed
        } else {
            GithubCheckStatusState::InProgress
        }
    }

    pub fn conclusion(&self) -> GithubCheckConclusionState {
        if self.status() == GithubCheckStatusState::InProgress {
            GithubCheckConclusionState::Neutral
        } else if self.failed_checks_count > 0 {
            GithubCheckConclusionState::Failure
        } else {
            GithubCheckConclusionState::Success
        }
    }
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
fn ReviewsGithubPullRequest(
    github_pull_request: ReadSignal<GithubPullRequest>,
    expand_details: ReadSignal<bool>,
) -> Element {
    let reviews_state = github_pull_request()
        .review_decision
        .as_ref()
        .map(|review_decision| match review_decision {
            GithubPullRequestReviewDecision::Approved => (
                "Pull request approved",
                rsx! { Icon { class: "h-5 w-5 text-success", icon: BsCheckCircleFill } },
            ),
            GithubPullRequestReviewDecision::ChangesRequested => (
                "Changes requested",
                rsx! { Icon { class: "h-5 w-5 text-error", icon: BsXCircleFill } },
            ),
            GithubPullRequestReviewDecision::ReviewRequired => (
                "Waiting for review",
                rsx! { Icon { class: "h-5 w-5 text-info", icon: BsPauseCircleFill } },
            ),
        })
        .unwrap_or_else(|| {
            (
                "Waiting for review",
                rsx! { Icon { class: "h-5 w-5 text-info", icon: BsPauseCircleFill } },
            )
        });

    rsx! {
        CollapseCardWithIcon {
            id: "reviews-github-pull-request",
            title: "{reviews_state.0}",
            icon: reviews_state.1,
            opened: expand_details(),
            ReviewsGithubPullRequestDetails { github_pull_request, expand_details }
        }
    }
}

#[component]
fn ReviewsGithubPullRequestDetails(
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

    rsx! {
        table {
            class: "table table-auto table-xs w-full",
            tbody {
                for review in reviews {
                    GithubReviewLine { review, expand_details }
                }
            }
        }
    }
}

#[component]
fn GithubReviewLine(review: GithubReview, expand_details: ReadSignal<bool>) -> Element {
    let id = use_memo(|| Uuid::new_v4().to_string())();
    let (reviewer, review_body, review_status_icon) = match review {
        GithubReview::Requested { reviewer } => (
            reviewer,
            None,
            rsx! { Icon { class: "h-5 w-5 text-info", icon: BsClock } },
        ),
        GithubReview::Completed {
            reviewer,
            state,
            body,
        } => (
            reviewer,
            (!body.is_empty()).then_some(body),
            match state {
                GithubPullRequestReviewState::Approved => {
                    rsx! { Icon { class: "h-5 w-5 text-success", icon: BsCheckCircleFill } }
                }
                GithubPullRequestReviewState::ChangesRequested => {
                    rsx! { Icon { class: "h-5 w-5 text-error", icon: BsXCircleFill } }
                }
                GithubPullRequestReviewState::Commented => {
                    rsx! { Icon { class: "h-5 w-5 text-info", icon: BsChatTextFill } }
                }
                _ => {
                    rsx! { Icon { class: "h-5 w-5 text-neutral", icon: BsQuestionCircleFill } }
                }
            },
        ),
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

    rsx! {
        tr {
            td {
                if let Some(review_body) = review_body {
                    Collapse {
                        id: "github-review-{id}",
                        opened: expand_details(),
                        header: rsx! {
                            div {
                                class: "flex gap-2 items-center",

                                { review_status_icon }

                                UserWithAvatar {
                                    user_name: reviewer_display_name.clone(),
                                    avatar_url: reviewer_avatar_url,
                                    display_name: true,
                                },
                            }
                        },

                        div {
                            class: "bg-neutral text-neutral-content p-2 my-1 rounded-sm",
                            dangerous_inner_html: "{review_body}"
                        }
                    }
                } else {
                    div {
                        class: "flex gap-2 items-center p-2",

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
    rsx! {
        div {
            class: "flex flex-col gap-2",
            for comment in comments() {
                CardWithHeaders {
                    headers: if let Some((user_name, avatar_url)) = comment
                        .author
                        .map(get_github_actor_name_and_url) {
                        vec![rsx! {
                            MessageHeader {
                                user_name,
                                avatar_url: Some(avatar_url),
                                display_name: true,
                                sent_at: comment.created_at
                            }
                        }]
                    } else { vec![] },
                    span { class: "prose prose-sm", dangerous_inner_html: "{comment.body}" }
                }
            }
        }
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
