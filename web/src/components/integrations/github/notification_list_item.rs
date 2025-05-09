#![allow(non_snake_case)]

use chrono::{DateTime, Local};
use dioxus::prelude::*;

use dioxus_free_icons::{icons::bs_icons::BsChatTextFill, Icon};
use universal_inbox::{
    notification::NotificationWithTask,
    third_party::integrations::github::{
        GithubDiscussion, GithubNotification, GithubNotificationItem, GithubPullRequest,
        GithubPullRequestReviewDecision,
    },
    HasHtmlUrl,
};

use crate::components::{
    integrations::github::{
        icons::{Github, GithubNotificationIcon},
        preview::pull_request::ChecksGithubPullRequest,
        GithubActorDisplay,
    },
    list::{ListContext, ListItem},
    notifications_list::{get_notification_list_item_action_buttons, TaskHint},
};

#[component]
pub fn GithubNotificationListItem(
    notification: ReadOnlySignal<NotificationWithTask>,
    github_notification: ReadOnlySignal<GithubNotification>,
    is_selected: ReadOnlySignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    match github_notification() {
        GithubNotification {
            item: Some(GithubNotificationItem::GithubPullRequest(github_pull_request)),
            ..
        } => rsx! {
            GithubPullRequestNotificationListItem {
                notification,
                github_notification,
                github_pull_request,
                is_selected,
                on_select,
            }
        },
        GithubNotification {
            item: Some(GithubNotificationItem::GithubDiscussion(github_discussion)),
            ..
        } => rsx! {
            GithubDiscussionNotificationListItem {
                notification,
                github_notification,
                github_discussion,
                is_selected,
                on_select,
            }
        },
        _ => rsx! {
            DefaultGithubNotificationListItem {
                notification,
                github_notification,
                is_selected,
                on_select,
            }
        },
    }
}

#[component]
pub fn DefaultGithubNotificationListItem(
    notification: ReadOnlySignal<NotificationWithTask>,
    github_notification: ReadOnlySignal<GithubNotification>,
    is_selected: ReadOnlySignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let notification_updated_at = use_memo(move || {
        Into::<DateTime<Local>>::into(notification().updated_at)
            .format("%Y-%m-%d %H:%M")
            .to_string()
    });
    let list_context = use_context::<Memo<ListContext>>();
    let link = notification().get_html_url();

    rsx! {
        ListItem {
            key: "{notification().id}",
            title: "{notification().title}",
            subtitle: rsx! { GithubNotificationSubtitle { github_notification } },
            link,
            icon: rsx! {
                Github { class: "h-5 w-5" },
                TaskHint { task: notification().task }
            },
            subicon: rsx! {
                GithubNotificationIcon {
                    class: "h-5 w-5 min-w-5",
                    notif: notification,
                    github_notification: github_notification
                }
            },
            action_buttons: get_notification_list_item_action_buttons(
                notification,
                list_context().show_shortcut,
                None,
                None
            ),
            is_selected,
            on_select,

            span { class: "text-base-content/50 whitespace-nowrap text-xs font-mono", "{notification_updated_at}" }
        }
    }
}

#[component]
pub fn GithubPullRequestNotificationListItem(
    notification: ReadOnlySignal<NotificationWithTask>,
    github_notification: ReadOnlySignal<GithubNotification>,
    github_pull_request: ReadOnlySignal<GithubPullRequest>,
    is_selected: ReadOnlySignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let notification_updated_at = use_memo(move || {
        Into::<DateTime<Local>>::into(notification().updated_at)
            .format("%Y-%m-%d %H:%M")
            .to_string()
    });
    let list_context = use_context::<Memo<ListContext>>();
    let link = notification().get_html_url();

    rsx! {
        ListItem {
            key: "{notification().id}",
            title: "{notification().title}",
            subtitle: rsx! { GithubNotificationSubtitle { github_notification } },
            link,
            icon: rsx! {
                Github { class: "h-5 w-5" },
                TaskHint { task: notification().task }
            },
            subicon: rsx! {
                GithubNotificationIcon {
                    class: "h-5 w-5 min-w-5",
                    notif: notification,
                    github_notification: github_notification
                }
            },
            action_buttons: get_notification_list_item_action_buttons(
                notification,
                list_context().show_shortcut,
                None,
                None
            ),
            is_selected,
            on_select,

            ChecksGithubPullRequest {
                icon_size: "h-5 w-5",
                latest_commit: github_pull_request().latest_commit,
                expand_details: false,
            }

            if github_pull_request().comments_count > 0 {
                div {
                    class: "flex gap-1 items-center",
                    Icon { class: "h-5 w-5 text-info", icon: BsChatTextFill }
                    span { class: "text-xs text-base-content/50", "{github_pull_request().comments_count}" }
                }
            }

            GithubReviewStatus { github_pull_request }

            if let Some(actor) = github_pull_request().author {
                GithubActorDisplay { actor }
            }

            span { class: "text-base-content/50 whitespace-nowrap text-xs font-mono", "{notification_updated_at}" }
        }
    }
}

#[component]
pub fn GithubDiscussionNotificationListItem(
    notification: ReadOnlySignal<NotificationWithTask>,
    github_notification: ReadOnlySignal<GithubNotification>,
    github_discussion: ReadOnlySignal<GithubDiscussion>,
    is_selected: ReadOnlySignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let notification_updated_at = use_memo(move || {
        Into::<DateTime<Local>>::into(notification().updated_at)
            .format("%Y-%m-%d %H:%M")
            .to_string()
    });
    let list_context = use_context::<Memo<ListContext>>();
    let link = notification().get_html_url();

    rsx! {
        ListItem {
            key: "{notification().id}",
            title: "{notification().title}",
            subtitle: rsx! { GithubNotificationSubtitle { github_notification } },
            link,
            icon: rsx! {
                Github { class: "h-5 w-5" },
                TaskHint { task: notification().task }
            },
            subicon: rsx! {
                GithubNotificationIcon {
                    class: "h-5 w-5 min-w-5",
                    notif: notification,
                    github_notification: github_notification
                }
            },
            action_buttons: get_notification_list_item_action_buttons(
                notification,
                list_context().show_shortcut,
                None,
                None
            ),
            is_selected,
            on_select,

            if github_discussion().comments_count > 0 {
                div {
                    class: "flex gap-1 items-center",
                    Icon { class: "h-5 w-5 text-info", icon: BsChatTextFill }
                    span { class: "text-xs text-base-content/50", "{github_discussion().comments_count}" }
                }
            }

            if let Some(actor) = github_discussion().author {
                GithubActorDisplay { actor }
            }

            span { class: "text-base-content/50 whitespace-nowrap text-xs font-mono", "{notification_updated_at}" }
        }
    }
}

#[component]
fn GithubNotificationSubtitle(github_notification: ReadOnlySignal<GithubNotification>) -> Element {
    rsx! {
        div {
            class: "flex gap-2 text-xs text-base-content/50",

            span { "{github_notification().repository.full_name}" }
            if let Some(github_notification_id) = github_notification().extract_id() {
                span { "#{github_notification_id}" }
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
                rsx! { div { class: "badge badge-sm p-1 whitespace-nowrap text-xs badge-success", "Approved" } }
            }
            GithubPullRequestReviewDecision::ChangesRequested => {
                rsx! { div { class: "badge badge-sm p-1 whitespace-nowrap text-xs basge-error", "Changes requested" } }
            }
            GithubPullRequestReviewDecision::ReviewRequired => {
                rsx! { div { class: "badge badge-sm p-1 whitespace-nowrap text-xs badge-info", "Review required" } }
            }
        })
        .unwrap_or_else(|| rsx! {})
}
