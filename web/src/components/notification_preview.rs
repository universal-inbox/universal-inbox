#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{
    notification::{NotificationId, NotificationWithTask},
    third_party::{
        integrations::{
            github::GithubNotificationItem,
            slack::{SlackReactionItem, SlackStarItem},
        },
        item::ThirdPartyItemData,
    },
};

use crate::{
    components::{
        integrations::{
            github::preview::{
                discussion::GithubDiscussionPreview, pull_request::GithubPullRequestPreview,
                GithubNotificationDefaultPreview,
            },
            google_mail::preview::GoogleMailThreadPreview,
            icons::{NotificationIcon, TaskIcon},
            linear::preview::LinearNotificationPreview,
            slack::preview::{
                channel::SlackChannelPreview, file::SlackFilePreview,
                file_comment::SlackFileCommentPreview, group::SlackGroupPreview,
                im::SlackImPreview, message::SlackMessagePreview,
            },
        },
        task_preview::TaskDetailsPreview,
    },
    model::{PreviewPane, UniversalInboxUIModel},
};

#[component]
pub fn NotificationPreview(
    ui_model: Signal<UniversalInboxUIModel>,
    notification: ReadOnlySignal<NotificationWithTask>,
) -> Element {
    let has_notification_details_preview = !notification().is_built_from_task();
    let has_task_details_preview = notification().task.is_some();

    let mut latest_shown_notification_id = use_signal(|| None::<NotificationId>);
    // reset selected_preview_pane when showing another notification
    if latest_shown_notification_id() != Some(notification().id) {
        ui_model.write().selected_preview_pane = if has_notification_details_preview {
            PreviewPane::Notification
        } else {
            PreviewPane::Task
        };
        *latest_shown_notification_id.write() = Some(notification().id);
    }

    let (notification_tab_style, task_tab_style) =
        if ui_model.read().selected_preview_pane == PreviewPane::Notification {
            ("tab-active", "")
        } else {
            ("", "tab-active")
        };

    rsx! {
        div {
            class: "flex flex-col gap-4 w-full",

            div {
                class: "tabs tabs-bordered w-full",
                role: "tablist",

                if has_notification_details_preview {
                    button {
                        class: "tab {notification_tab_style}",
                        role: "tab",
                        onclick: move |_| { ui_model.write().selected_preview_pane = PreviewPane::Notification },
                        div {
                            class: "flex gap-2",
                            NotificationIcon { class: "h-5 w-5", kind: notification().kind }
                            "Notification"
                        }
                    }
                }
                if has_task_details_preview {
                    button {
                        class: "tab {task_tab_style}",
                        role: "tab",
                        onclick: move |_| { ui_model.write().selected_preview_pane = PreviewPane::Task },
                        div {
                            class: "flex gap-2",
                            if let Some(task) = notification().task {
                                TaskIcon { class: "h-5 w-5", _kind: task.kind }
                            }
                            "Task"
                        }
                    }
                }
            }

            match ui_model.read().selected_preview_pane {
                PreviewPane::Notification => rsx! { NotificationDetailsPreview { notification } },
                PreviewPane::Task => rsx! {
                    if let Some(task) = notification().task {
                        TaskDetailsPreview { task }
                    }
                },
            }
        }
    }
}

#[component]
fn NotificationDetailsPreview(notification: ReadOnlySignal<NotificationWithTask>) -> Element {
    match notification().source_item.data {
        ThirdPartyItemData::GithubNotification(github_notification) => {
            match github_notification.item {
                Some(GithubNotificationItem::GithubPullRequest(github_pull_request)) => {
                    rsx! { GithubPullRequestPreview { github_pull_request } }
                }
                Some(GithubNotificationItem::GithubDiscussion(github_discussion)) => {
                    rsx! { GithubDiscussionPreview { github_discussion } }
                }
                _ => rsx! {
                    GithubNotificationDefaultPreview {
                        notification,
                        github_notification: *github_notification
                    }
                },
            }
        }
        ThirdPartyItemData::SlackReaction(slack_reaction) => match slack_reaction.item {
            SlackReactionItem::SlackMessage(slack_message) => {
                rsx! { SlackMessagePreview { slack_message, title: notification().title } }
            }
            SlackReactionItem::SlackFile(slack_file) => {
                rsx! { SlackFilePreview { slack_file, title: notification().title } }
            }
        },
        ThirdPartyItemData::SlackStar(slack_star) => match slack_star.item {
            SlackStarItem::SlackMessage(slack_message) => {
                rsx! { SlackMessagePreview { slack_message, title: notification().title } }
            }
            SlackStarItem::SlackFile(slack_file) => {
                rsx! { SlackFilePreview { slack_file, title: notification().title } }
            }
            SlackStarItem::SlackFileComment(slack_file_comment) => rsx! {
                SlackFileCommentPreview { slack_file_comment, title: notification().title }
            },
            SlackStarItem::SlackChannel(slack_channel) => {
                rsx! { SlackChannelPreview { slack_channel, title: notification().title } }
            }
            SlackStarItem::SlackIm(slack_im) => {
                rsx! { SlackImPreview { slack_im, title: notification().title } }
            }
            SlackStarItem::SlackGroup(slack_group) => {
                rsx! { SlackGroupPreview { slack_group, title: notification().title } }
            }
        },
        ThirdPartyItemData::LinearNotification(linear_notification) => rsx! {
            LinearNotificationPreview { linear_notification: *linear_notification }
        },
        ThirdPartyItemData::GoogleMailThread(google_mail_thread) => rsx! {
            GoogleMailThreadPreview {
                notification,
                google_mail_thread: *google_mail_thread
            }
        },
        _ => None,
    }
}
