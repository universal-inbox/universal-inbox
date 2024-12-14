#![allow(non_snake_case)]

use dioxus::prelude::*;

use dioxus_free_icons::{icons::bs_icons::BsChatTextFill, Icon};
use universal_inbox::{
    notification::NotificationWithTask,
    third_party::integrations::github::{
        GithubDiscussion, GithubNotification, GithubPullRequest, GithubPullRequestReviewDecision,
    },
};

use crate::components::{
    integrations::github::{
        icons::GithubNotificationIcon, preview::pull_request::ChecksGithubPullRequest,
        GithubActorDisplay,
    },
    markdown::Markdown,
};

#[component]
pub fn GithubNotificationDisplay(
    notif: ReadOnlySignal<NotificationWithTask>,
    github_notification: ReadOnlySignal<GithubNotification>,
) -> Element {
    let github_notification_id = github_notification().extract_id();

    rsx! {
        div {
            class: "flex items-center gap-2",

            GithubNotificationIcon {
                class: "h-5 w-5 min-w-5",
                notif: notif,
                github_notification: github_notification
            }

            div {
                class: "flex flex-col grow",

                Markdown { text: notif().title.clone() }
                div {
                    class: "flex gap-2 text-xs text-gray-400",

                    span { "{github_notification().repository.full_name}" }
                    if let Some(github_notification_id) = github_notification_id {
                        span { "#{github_notification_id}" }
                    }
                }
            }
        }
    }
}

#[component]
pub fn GithubPullRequestDetailsDisplay(
    github_pull_request: ReadOnlySignal<GithubPullRequest>,
    expand_details: ReadOnlySignal<bool>,
) -> Element {
    rsx! {
        div {
            class: "flex items-center gap-2",

            ChecksGithubPullRequest {
                latest_commit: github_pull_request().latest_commit,
                icon_size: "h-3 w-3",
                expand_details
            }

            if github_pull_request().comments_count > 0 {
                div {
                    class: "flex gap-1",
                    Icon { class: "h-3 w-3 text-info", icon: BsChatTextFill }
                    span { class: "text-xs text-gray-400", "{github_pull_request().comments_count}" }
                }
            }

            GithubReviewStatus { github_pull_request: github_pull_request }

            if let Some(actor) = &github_pull_request().author {
                GithubActorDisplay { actor: actor.clone() }
            }
        }
    }
}

#[component]
pub fn GithubReviewStatus(github_pull_request: ReadOnlySignal<GithubPullRequest>) -> Element {
    github_pull_request()
        .review_decision
        .as_ref()
        .map(|review_decision| match review_decision {
            GithubPullRequestReviewDecision::Approved => {
                rsx! { div { class: "badge p-1 whitespace-nowrap bg-success text-xs text-white", "Approved" } }
            }
            GithubPullRequestReviewDecision::ChangesRequested => {
                rsx! { div { class: "badge p-1 whitespace-nowrap bg-error text-xs text-white", "Changes requested" } }
            }
            GithubPullRequestReviewDecision::ReviewRequired => {
                rsx! { div { class: "badge p-1 whitespace-nowrap bg-info text-xs text-white", "Review required" } }
            }
        })
        .unwrap_or(None)
}

#[component]
pub fn GithubDiscussionDetailsDisplay(
    github_discussion: ReadOnlySignal<GithubDiscussion>,
) -> Element {
    rsx! {
        div {
            class: "flex items-center gap-2",

            if github_discussion().comments_count > 0 {
                div {
                    class: "flex gap-1",
                    Icon { class: "h-3 w-3 text-info", icon: BsChatTextFill }
                    span { class: "text-xs text-gray-400", "{github_discussion().comments_count}" }
                }
            }

            if let Some(actor) = &github_discussion().author {
                GithubActorDisplay { actor: actor.clone() }
            }
        }
    }
}
