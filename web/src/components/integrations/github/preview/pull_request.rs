#![allow(non_snake_case)]

use std::{collections::HashMap, default::Default};

use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{
        BsArrowUpRightSquare, BsChatTextFill, BsCheckCircleFill, BsPauseCircleFill,
        BsQuestionCircleFill, BsSkipForwardCircleFill, BsXCircleFill,
    },
    Icon,
};
use itertools::Itertools;

use universal_inbox::notification::integrations::github::{
    GithubActor, GithubBotSummary, GithubCheckConclusionState, GithubCheckRun,
    GithubCheckStatusState, GithubCheckSuite, GithubCheckSuiteApp, GithubCommitChecks, GithubLabel,
    GithubMannequinSummary, GithubMergeableState, GithubPullRequest, GithubPullRequestReview,
    GithubPullRequestReviewDecision, GithubPullRequestReviewState, GithubPullRequestState,
    GithubRepositorySummary, GithubReviewer, GithubTeamSummary, GithubUserSummary, GithubWorkflow,
};

use crate::components::{
    integrations::github::{icons::GithubPullRequestIcon, GithubActorDisplay},
    CollapseCardWithIcon, SmallCard, Tag, TagsInCard, UserWithAvatar,
};

#[inline_props]
pub fn GithubPullRequestPreview<'a>(
    cx: Scope,
    github_pull_request: &'a GithubPullRequest,
) -> Element {
    let show_base_and_head_repositories = match (
        &github_pull_request.head_repository,
        &github_pull_request.base_repository,
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

    render! {
        div {
            class: "flex flex-col w-full gap-2",

            div {
                class: "flex gap-2",

                if let Some(head_repository) = &github_pull_request.head_repository {
                    render! {
                        a {
                            class: "text-xs text-gray-400",
                            href: "{head_repository.url}",
                            target: "_blank",
                            "{head_repository.name_with_owner}"
                        }
                    }
                }

                a {
                    class: "text-xs text-gray-400",
                    href: "{github_pull_request.url}",
                    target: "_blank",
                    "#{github_pull_request.number}"
                }
            }

            h2 {
                class: "flex items-center gap-2 text-lg",

                GithubPullRequestIcon { class: "h-5 w-5", github_pull_request: github_pull_request }
                a {
                    href: "{github_pull_request.url}",
                    target: "_blank",
                    dangerous_inner_html: "{github_pull_request.title}"
                }
                a {
                    class: "flex-none",
                    href: "{github_pull_request.url}",
                    target: "_blank",
                    Icon { class: "h-5 w-5 text-gray-400 p-1", icon: BsArrowUpRightSquare }
                }
            }

            div {
                class: "flex flex-wrap items-center text-gray-400 gap-1 text-xs",

                "From"
                if show_base_and_head_repositories {
                    render! {
                        if let Some(head_repository) = &github_pull_request.head_repository {
                            render! {
                                a {
                                    href: "{head_repository.url}",
                                    target: "_blank",
                                    "{head_repository.name_with_owner}:"
                                }
                            }
                        }
                    }
                }
                span { class: "text-primary", "{github_pull_request.head_ref_name}" }

                "into"

                if show_base_and_head_repositories {
                    render! {
                        if let Some(base_repository) = &github_pull_request.base_repository {
                            render! {
                                a {
                                    href: "{base_repository.url}",
                                    target: "_blank",
                                    "{base_repository.name_with_owner}:"
                                }
                            }
                        }
                    }
                }
                span { class: "text-primary", "{github_pull_request.base_ref_name}" }
            }

            GithubPullRequestDetails { github_pull_request: github_pull_request }
        }
    }
}

impl From<GithubLabel> for Tag {
    fn from(github_label: GithubLabel) -> Self {
        Tag {
            name: github_label.name,
            color: Some(github_label.color),
        }
    }
}

#[inline_props]
fn GithubPullRequestDetails<'a>(cx: Scope, github_pull_request: &'a GithubPullRequest) -> Element {
    let pr_state_label = match github_pull_request.state {
        GithubPullRequestState::Closed => "Closed",
        GithubPullRequestState::Merged => "Merged",
        GithubPullRequestState::Open => {
            if github_pull_request.is_draft {
                "Draft"
            } else {
                "Opened"
            }
        }
    };

    let (mergeable_state_label, mergeable_state_icon) = match github_pull_request.mergeable_state {
        GithubMergeableState::Mergeable => (
            "Pull request is mergeable",
            render! { Icon { class: "h-5 w-5 text-success", icon: BsCheckCircleFill } },
        ),
        GithubMergeableState::Conflicting => (
            "Pull request is conflicting",
            render! { Icon { class: "h-5 w-5 text-error", icon: BsXCircleFill } },
        ),
        GithubMergeableState::Unknown => (
            "Unknown pull request mergeable state",
            render! { Icon { class: "h-5 w-5 text-warning", icon: BsQuestionCircleFill } },
        ),
    };

    render! {
        div {
            class: "flex flex-col gap-2 w-full",

            TagsInCard {
                tags: github_pull_request
                    .labels
                    .iter()
                    .map(|label| label.clone().into())
                    .collect()
            }

            if let Some(actor) = &github_pull_request.author {
                render! {
                    SmallCard {
                        span { class: "text-gray-400", "Opened by" }
                        GithubActorDisplay { actor: actor }
                    }
                }
            }

            if !github_pull_request.assignees.is_empty() {
                render! {
                    SmallCard {
                        span { class: "text-gray-400", "Assigned to" }
                        div {
                            class: "flex flex-col",
                            for assignee in &github_pull_request.assignees {
                                render! {
                                    GithubActorDisplay { actor: assignee }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(merged_by) = &github_pull_request.merged_by {
                render! {
                    SmallCard {
                        span { class: "text-gray-400", "Merged by" }
                        GithubActorDisplay { actor: merged_by }
                    }
                }
            }

            SmallCard {
                GithubPullRequestIcon { class: "h-5 w-5", github_pull_request: github_pull_request }
                span { "{pr_state_label}" }
            }

            if github_pull_request.state == GithubPullRequestState::Open {
                render! {
                    SmallCard { mergeable_state_icon, span { "{mergeable_state_label}" } }
                }
            }

            ChecksGithubPullRequest { latest_commit: &github_pull_request.latest_commit }

            ReviewsGithubPullRequest { github_pull_request: github_pull_request }

            p {
                class: "w-full prose prose-sm dark:prose-invert",
                dangerous_inner_html: "{github_pull_request.body}"
            }
        }
    }
}

#[inline_props]
fn ChecksGithubPullRequest<'a>(cx: Scope, latest_commit: &'a GithubCommitChecks) -> Element {
    let checks_progress = use_memo(
        cx,
        &(latest_commit.check_suites.clone(),),
        |(check_suites,)| compute_pull_request_checks_progress(&check_suites),
    );

    let checks_state = checks_progress.as_ref().map(|checks_progress| {
        match checks_progress.status() {
            GithubCheckStatusState::Pending => (
                "Checks not started yet",
                render! { Icon { class: "h-5 w-5 text-success", icon: BsPauseCircleFill } },
            ),
            GithubCheckStatusState::InProgress => (
                "Checks are in progress",
                render! { span { class: "h-5 w-5 loading loading-spinner text-primary" } },
            ),
            GithubCheckStatusState::Completed => match checks_progress.conclusion() {
                GithubCheckConclusionState::Success => (
                    "All checks passed",
                    render! { Icon { class: "h-5 w-5 text-success", icon: BsCheckCircleFill } },
                ),
                GithubCheckConclusionState::Failure => (
                    "Some checks failed",
                    render! { Icon { class: "h-5 w-5 text-error", icon: BsXCircleFill } },
                ),
                _ => (
                    "Unknown checks status",
                    render! { Icon { class: "h-5 w-5 text-warning", icon: BsQuestionCircleFill } },
                ),
            },
            _ => (
                "Unknown checks status",
                render! { Icon { class: "h-5 w-5 text-warning", icon: BsQuestionCircleFill } },
            ),
        }
    });

    if let Some(checks_state) = checks_state {
        render! {
            CollapseCardWithIcon {
                title: "{checks_state.0}",
                icon: render! { checks_state.1 },
                ChecksGithubPullRequestDetails { latest_commit: latest_commit }
            }
        }
    } else {
        None
    }
}

#[inline_props]
fn ChecksGithubPullRequestDetails<'a>(cx: Scope, latest_commit: &'a GithubCommitChecks) -> Element {
    if let Some(check_suites) = &latest_commit.check_suites {
        render! {
            table {
                class: "table table-auto table-xs w-full",
                tbody {
                    for check_suite in check_suites {
                        if check_suite.status != GithubCheckStatusState::Queued {
                            render! {
                                for check_run in check_suite.check_runs.iter() {
                                    render! {
                                        GithubCheckRunLine {
                                            check_run: check_run,
                                            workflow: &check_suite.workflow,
                                            app: &check_suite.app,
                                        }
                                    }
                                }
                            }
                        } else {
                            None
                        }
                    }
                }
            }
        }
    } else {
        None
    }
}

#[inline_props]
fn GithubCheckRunLine<'a>(
    cx: Scope,
    check_run: &'a GithubCheckRun,
    workflow: &'a Option<GithubWorkflow>,
    app: &'a Option<GithubCheckSuiteApp>,
) -> Element {
    let check_run_status_icon = match check_run.status {
        GithubCheckStatusState::Completed => match check_run.conclusion {
            Some(GithubCheckConclusionState::Success) => {
                render! { Icon { class: "h-5 w-5 text-success", icon: BsCheckCircleFill } }
            }
            Some(GithubCheckConclusionState::Failure) => {
                render! { Icon { class: "h-5 w-5 text-error", icon: BsXCircleFill } }
            }
            _ => render! { Icon { class: "h-5 w-5 text-warning", icon: BsQuestionCircleFill } },
        },
        GithubCheckStatusState::InProgress => {
            render! { span { class: "h-5 w-5 loading loading-spinner text-primary" } }
        }
        GithubCheckStatusState::Pending => {
            render! { Icon { class: "h-5 w-5 text-base-content", icon: BsPauseCircleFill } }
        }
        GithubCheckStatusState::Queued => {
            render! { Icon { class: "h-5 w-5 text-base-content", icon: BsSkipForwardCircleFill } }
        }
        GithubCheckStatusState::Requested => {
            render! { Icon { class: "h-5 w-5 text-base-content", icon: BsQuestionCircleFill } }
        }
        GithubCheckStatusState::Waiting => {
            render! { Icon { class: "h-5 w-5 text-base-content", icon: BsPauseCircleFill } }
        }
    };

    render! {
        tr {
            td {
                div {
                    class: "flex items-center gap-1",
                    check_run_status_icon

                    if let Some(app) = app {
                        render! {
                            a {
                                href: "{app.url}",
                                target: "_blank",

                                img {
                                    class: "h-5 w-5 rounded-full",
                                    alt: "{app.name}",
                                    title: "{app.name}",
                                    src: "{app.logo_url}"
                                }
                            }
                        }
                    } else {
                        render! { Icon { class: "h-5 w-5", icon: BsQuestionCircleFill } }
                    }

                    if let Some(workflow) = workflow {
                        render! {
                            a {
                                class: "text-primary",
                                href: "{workflow.url}",
                                target: "_blank",
                                "{workflow.name}"
                            }
                        }
                    } else if let Some(check_run_url) = &check_run.url {
                        render! {
                            a {
                                class: "text-primary",
                                href: "{check_run_url}",
                                target: "_blank",
                                "{check_run.name}"
                            }
                        }
                    } else {
                        render! { span { class: "text-primary", "{check_run.name}" } }
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
                        if let Some(conclusion) = &check_run.conclusion {
                            if *conclusion != GithubCheckConclusionState::Success {
                                progress.failed_checks_count += 1;
                            }
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

#[inline_props]
fn ReviewsGithubPullRequest<'a>(cx: Scope, github_pull_request: &'a GithubPullRequest) -> Element {
    let reviews_state = github_pull_request
        .review_decision
        .as_ref()
        .map(|review_decision| match review_decision {
            GithubPullRequestReviewDecision::Approved => (
                "Pull request approved",
                render! { Icon { class: "h-5 w-5 text-success", icon: BsCheckCircleFill } },
            ),
            GithubPullRequestReviewDecision::ChangesRequested => (
                "Changes requested",
                render! { Icon { class: "h-5 w-5 text-error", icon: BsXCircleFill } },
            ),
            GithubPullRequestReviewDecision::ReviewRequired => (
                "Waiting for review",
                render! { Icon { class: "h-5 w-5 text-info", icon: BsPauseCircleFill } },
            ),
        });

    if let Some(reviews_state) = reviews_state {
        render! {
            CollapseCardWithIcon {
                title: "{reviews_state.0}",
                icon: render! { reviews_state.1 },
                ReviewsGithubPullRequestDetails { github_pull_request: github_pull_request }
            }
        }
    } else {
        None
    }
}

#[inline_props]
fn ReviewsGithubPullRequestDetails<'a>(
    cx: Scope,
    github_pull_request: &'a GithubPullRequest,
) -> Element {
    let reviews = compute_pull_request_reviews(
        github_pull_request.reviews.as_ref(),
        github_pull_request.review_requests.as_ref(),
    );
    render! {
        table {
            class: "table table-auto table-xs w-full",
            tbody {
                for review in reviews {
                    render! {
                        GithubReviewLine { review: review }
                    }
                }
            }
        }
    }
}

#[inline_props]
fn GithubReviewLine(cx: Scope, review: GithubReview) -> Element {
    let (reviewer, review_body, review_status_icon) = match review {
        GithubReview::Requested { reviewer } => (
            reviewer,
            None,
            render! { Icon { class: "h-5 w-5 text-info", icon: BsPauseCircleFill } }, // TODO clock icon
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
                    render! { Icon { class: "h-5 w-5 text-success", icon: BsCheckCircleFill } }
                }
                GithubPullRequestReviewState::ChangesRequested => {
                    render! { Icon { class: "h-5 w-5 text-error", icon: BsXCircleFill } }
                }
                GithubPullRequestReviewState::Commented => {
                    render! { Icon { class: "h-5 w-5 text-info", icon: BsChatTextFill } }
                }
                _ => {
                    render! { Icon { class: "h-5 w-5 text-neutral", icon: BsQuestionCircleFill } }
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

    render! {
        tr {
            td {
                if let Some(review_body) = review_body {
                    render! {
                        details {
                            class: "collapse collapse-arrow",
                            summary {
                                class: "collapse-title min-h-min py-2 px-0",
                                div {
                                    class: "flex gap-2 items-center",
                                    review_status_icon,
                                    UserWithAvatar {
                                        user_name: reviewer_display_name,
                                        avatar_url: reviewer_avatar_url

                                    },
                                }
                            }

                            div {
                                class: "bg-neutral text-neutral-content p-2 my-1 rounded",
                                dangerous_inner_html: "{review_body}"
                            }
                        }
                    }
                } else {
                    render! {
                        div {
                            class: "flex gap-2 items-center",

                            review_status_icon,
                            UserWithAvatar {
                                user_name: reviewer_display_name,
                                avatar_url: reviewer_avatar_url
                            },
                        }
                    }
                }
            }
        }
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
enum GithubReview {
    Requested {
        reviewer: GithubReviewer,
    },
    Completed {
        reviewer: GithubReviewer,
        body: String,
        state: GithubPullRequestReviewState,
    },
}

fn compute_pull_request_reviews(
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
                    &[requested_reviewer.clone()]
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
                    &[requested_reviewer.clone()]
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
