#![allow(non_snake_case)]

use dioxus::prelude::*;

use dioxus_free_icons::{icons::bs_icons::BsChatTextFill, Icon};
use universal_inbox::notification::{
    integrations::github::{
        GithubDiscussion, GithubNotification, GithubPullRequest, GithubPullRequestReviewDecision,
    },
    NotificationWithTask,
};

use crate::components::integrations::github::{
    icons::GithubNotificationIcon, preview::pull_request::ChecksGithubPullRequest,
    GithubActorDisplay,
};

#[component]
pub fn GithubNotificationDisplay<'a>(
    cx: Scope,
    notif: &'a NotificationWithTask,
    github_notification: &'a GithubNotification,
) -> Element {
    let github_notification_id = github_notification.extract_id();
    let title = markdown::to_html(&notif.title);

    render! {
        div {
            class: "flex items-center gap-2",

            GithubNotificationIcon {
                class: "h-5 w-5 min-w-5",
                notif: notif,
                github_notification: github_notification
            }

            div {
                class: "flex flex-col grow",

                span { dangerous_inner_html: "{title}" }
                div {
                    class: "flex gap-2 text-xs text-gray-400",

                    span { "{github_notification.repository.full_name}" }
                    if let Some(github_notification_id) = github_notification_id {
                        render! {
                            span { "#{github_notification_id}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn GithubPullRequestDetailsDisplay<'a>(
    cx: Scope,
    github_pull_request: &'a GithubPullRequest,
) -> Element {
    render! {
        div {
            class: "flex items-center gap-2",

            ChecksGithubPullRequest { latest_commit: &github_pull_request.latest_commit, icon_size: "h-3 w-3" }

            if github_pull_request.comments_count > 0 {
                render! {
                    div {
                        class: "flex gap-1",
                        Icon { class: "h-3 w-3 text-info", icon: BsChatTextFill }
                        span { class: "text-xs text-gray-400", "{github_pull_request.comments_count}" }
                    }
                }
            }

            GithubReviewStatus { github_pull_request: github_pull_request }

            if let Some(actor) = &github_pull_request.author {
                render! { GithubActorDisplay { actor: actor, without_name: true } }
            } else {
                None
            }
        }
    }
}

#[component]
fn GithubReviewStatus<'a>(cx: Scope, github_pull_request: &'a GithubPullRequest) -> Element {
    github_pull_request
        .review_decision
        .as_ref()
        .map(|review_decision| match review_decision {
            GithubPullRequestReviewDecision::Approved => {
                render! { div { class: "badge p-1 whitespace-nowrap bg-success text-xs text-white", "Approved" } }
            }
            GithubPullRequestReviewDecision::ChangesRequested => {
                render! { div { class: "badge p-1 whitespace-nowrap bg-error text-xs text-white", "Changes requested" } }
            }
            GithubPullRequestReviewDecision::ReviewRequired => {
                render! { div { class: "badge p-1 whitespace-nowrap bg-info text-xs text-white", "Review required" } }
            }
        })
        .unwrap_or(None)
}

#[component]
pub fn GithubDiscussionDetailsDisplay<'a>(
    cx: Scope,
    github_discussion: &'a GithubDiscussion,
) -> Element {
    render! {
        div {
            class: "flex items-center gap-2",

            if github_discussion.comments_count > 0 {
                render! {
                    div {
                        class: "flex gap-1",
                        Icon { class: "h-3 w-3 text-info", icon: BsChatTextFill }
                        span { class: "text-xs text-gray-400", "{github_discussion.comments_count}" }
                    }
                }
            }

            if let Some(actor) = &github_discussion.author {
                render! { GithubActorDisplay { actor: actor, without_name: true } }
            } else {
                None
            }
        }
    }
}
