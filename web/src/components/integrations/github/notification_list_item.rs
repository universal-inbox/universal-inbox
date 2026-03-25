#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{
    notification::{NotificationStatus, NotificationWithTask},
    third_party::integrations::github::{
        GithubDiscussion, GithubNotification, GithubNotificationItem, GithubPullRequest,
    },
};

use crate::{
    components::{
        integrations::github::icons::{Github, GithubPullRequestIcon},
        list::ListItem,
    },
    utils::format_elapsed_time,
};

#[component]
pub fn GithubNotificationListItem(
    notification: ReadSignal<NotificationWithTask>,
    github_notification: ReadSignal<GithubNotification>,
    is_selected: ReadSignal<bool>,
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
    notification: ReadSignal<NotificationWithTask>,
    github_notification: ReadSignal<GithubNotification>,
    is_selected: ReadSignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let notification_updated_at = use_memo(move || format_elapsed_time(notification().updated_at));
    let is_unread = notification().status == NotificationStatus::Unread;

    rsx! {
        ListItem {
            key: "{notification().id}",
            linked_task: notification().task,
            title: "{notification().title}",
            subtitle: rsx! { GithubNotificationSubtitle { github_notification } },
            time: "{notification_updated_at}",
            icon: rsx! {
                div {
                    class: "w-full h-full flex items-center justify-center rounded-[inherit] bg-[var(--ui-surface)] border border-[var(--ui-border)]",
                    Github { class: "h-4 w-4" }
                }
            },
            meta_icon: rsx! { span { class: "icon-[lucide--circle-dot] w-full h-full" } },
            is_selected,
            is_unread,
            provider: Some("github"),
            data_kind: Some("issue"),
            on_select,
        }
    }
}

#[component]
pub fn GithubPullRequestNotificationListItem(
    notification: ReadSignal<NotificationWithTask>,
    github_notification: ReadSignal<GithubNotification>,
    github_pull_request: ReadSignal<GithubPullRequest>,
    is_selected: ReadSignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let notification_updated_at = use_memo(move || format_elapsed_time(notification().updated_at));
    let is_unread = notification().status == NotificationStatus::Unread;

    rsx! {
        ListItem {
            key: "{notification().id}",
            linked_task: notification().task,
            title: "{notification().title}",
            subtitle: rsx! { GithubNotificationSubtitle { github_notification } },
            time: "{notification_updated_at}",
            icon: rsx! {
                div {
                    class: "w-full h-full flex items-center justify-center rounded-[inherit] bg-[var(--ui-surface)] border border-[var(--ui-border)]",
                    Github { class: "h-4 w-4" }
                }
            },
            meta_icon: rsx! {
                GithubPullRequestIcon {
                    class: "w-full h-full",
                    github_pull_request: github_pull_request(),
                }
            },
            is_selected,
            is_unread,
            provider: Some("github"),
            data_kind: Some("pull_request"),
            on_select,
        }
    }
}

#[component]
pub fn GithubDiscussionNotificationListItem(
    notification: ReadSignal<NotificationWithTask>,
    github_notification: ReadSignal<GithubNotification>,
    github_discussion: ReadSignal<GithubDiscussion>,
    is_selected: ReadSignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let notification_updated_at = use_memo(move || format_elapsed_time(notification().updated_at));
    let is_unread = notification().status == NotificationStatus::Unread;

    rsx! {
        ListItem {
            key: "{notification().id}",
            linked_task: notification().task,
            title: "{notification().title}",
            subtitle: rsx! { GithubNotificationSubtitle { github_notification } },
            time: "{notification_updated_at}",
            icon: rsx! {
                div {
                    class: "w-full h-full flex items-center justify-center rounded-[inherit] bg-[var(--ui-surface)] border border-[var(--ui-border)]",
                    Github { class: "h-4 w-4" }
                }
            },
            meta_icon: rsx! { span { class: "icon-[lucide--message-square] w-full h-full" } },
            is_selected,
            is_unread,
            provider: Some("github"),
            data_kind: Some("discussion"),
            on_select,
        }
    }
}

#[component]
fn GithubNotificationSubtitle(github_notification: ReadSignal<GithubNotification>) -> Element {
    rsx! {
        span {
            class: "ui-nrow-meta-text",
            "{github_notification().repository.full_name}"
            if let Some(github_notification_id) = github_notification().extract_id() {
                " #{github_notification_id}"
            }
        }
    }
}
