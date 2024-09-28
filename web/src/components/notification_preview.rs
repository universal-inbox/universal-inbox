#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::notification::{
    NotificationDetails, NotificationId, NotificationMetadata, NotificationWithTask,
};

use crate::{
    components::{
        integrations::{
            github::preview::{
                discussion::GithubDiscussionPreview, pull_request::GithubPullRequestPreview,
                GithubNotificationDefaultPreview,
            },
            google_mail::preview::GoogleMailThreadPreview,
            icons::{NotificationMetadataIcon, TaskIcon},
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
                            NotificationMetadataIcon { class: "h-5 w-5", notification_metadata: notification().metadata }
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
    if let Some(details) = notification().details {
        return match details {
            NotificationDetails::GithubPullRequest(github_pull_request) => rsx! {
                GithubPullRequestPreview { github_pull_request: github_pull_request }
            },
            NotificationDetails::GithubDiscussion(github_discussion) => rsx! {
                GithubDiscussionPreview { github_discussion: github_discussion }
            },
            NotificationDetails::SlackMessage(slack_message) => rsx! {
                SlackMessagePreview { slack_message, title: notification().title }
            },
            NotificationDetails::SlackFile(slack_file) => rsx! {
                SlackFilePreview { slack_file, title: notification().title }
            },
            NotificationDetails::SlackFileComment(slack_file_comment) => rsx! {
                SlackFileCommentPreview { slack_file_comment, title: notification().title }
            },
            NotificationDetails::SlackChannel(slack_channel) => rsx! {
                SlackChannelPreview { slack_channel, title: notification().title }
            },
            NotificationDetails::SlackIm(slack_im) => rsx! {
                SlackImPreview { slack_im, title: notification().title }
            },
            NotificationDetails::SlackGroup(slack_group) => rsx! {
                SlackGroupPreview { slack_group, title: notification().title }
            },
        };
    }
    match notification().metadata {
        NotificationMetadata::Github(github_notification) => rsx! {
            GithubNotificationDefaultPreview {
                notification: notification,
                github_notification: *github_notification,
            }
        },
        NotificationMetadata::Linear(linear_notification) => rsx! {
            LinearNotificationPreview { linear_notification: *linear_notification }
        },
        NotificationMetadata::GoogleMail(google_mail_thread) => rsx! {
            GoogleMailThreadPreview {
                notification: notification,
                google_mail_thread: *google_mail_thread.clone()
            }
        },
        NotificationMetadata::Slack(_) => None,
        NotificationMetadata::Todoist => None,
    }
}
