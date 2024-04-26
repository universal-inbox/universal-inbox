#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{
    notification::{
        NotificationDetails, NotificationId, NotificationMetadata, NotificationWithTask,
    },
    task::Task,
    third_party::item::{ThirdPartyItem, ThirdPartyItemData},
};

use crate::{
    components::integrations::{
        github::preview::{
            discussion::GithubDiscussionPreview, pull_request::GithubPullRequestPreview,
            GithubNotificationDefaultPreview,
        },
        google_mail::preview::GoogleMailThreadPreview,
        icons::{NotificationMetadataIcon, TaskIcon},
        linear::preview::LinearNotificationPreview,
        slack::preview::{
            channel::SlackChannelPreview, file::SlackFilePreview,
            file_comment::SlackFileCommentPreview, group::SlackGroupPreview, im::SlackImPreview,
            message::SlackMessagePreview,
        },
        todoist::preview::TodoistTaskPreview,
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
                PreviewPane::Notification => rsx! { NotificationDetailsPreview { notification: notification } },
                PreviewPane::Task => rsx! { TaskDetailsPreview { notification: notification } },
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
                SlackMessagePreview { notification: notification, slack_message: slack_message }
            },
            NotificationDetails::SlackFile(slack_file) => rsx! {
                SlackFilePreview { notification: notification, slack_file: slack_file }
            },
            NotificationDetails::SlackFileComment(slack_file_comment) => rsx! {
                SlackFileCommentPreview { notification: notification, slack_file_comment: slack_file_comment }
            },
            NotificationDetails::SlackChannel(slack_channel) => rsx! {
                SlackChannelPreview { notification: notification, slack_channel: slack_channel }
            },
            NotificationDetails::SlackIm(slack_im) => rsx! {
                SlackImPreview { notification: notification, slack_im: slack_im }
            },
            NotificationDetails::SlackGroup(slack_group) => rsx! {
                SlackGroupPreview { notification: notification, slack_group: slack_group }
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

#[component]
fn TaskDetailsPreview(notification: ReadOnlySignal<NotificationWithTask>) -> Element {
    match notification().task {
        Some(
            ref task @ Task {
                source_item:
                    ThirdPartyItem {
                        data: ThirdPartyItemData::TodoistItem(ref todoist_task),
                        ..
                    },
                ..
            },
        ) => rsx! {
            TodoistTaskPreview {
                notification: notification,
                task: task.clone(),
                todoist_task: todoist_task.clone(),
            }
        },
        _ => None,
    }
}
