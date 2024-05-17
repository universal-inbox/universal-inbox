#![allow(non_snake_case)]

use dioxus::prelude::*;
use fermi::UseAtomRef;

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
pub fn NotificationPreview<'a>(
    cx: Scope,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
    notification: &'a NotificationWithTask,
) -> Element {
    let has_notification_details_preview = !notification.is_built_from_task();
    let has_task_details_preview = notification.task.is_some();

    let latest_shown_notification_id = use_state(cx, || None::<NotificationId>);
    // reset selected_preview_pane when showing another notification
    if *latest_shown_notification_id != Some(notification.id) {
        ui_model_ref.write().selected_preview_pane = if has_notification_details_preview {
            PreviewPane::Notification
        } else {
            PreviewPane::Task
        };
        latest_shown_notification_id.set(Some(notification.id));
    }

    let (notification_tab_style, task_tab_style) =
        if ui_model_ref.read().selected_preview_pane == PreviewPane::Notification {
            ("tab-active", "")
        } else {
            ("", "tab-active")
        };

    render! {
        div {
            class: "flex flex-col gap-4 w-full",

            div {
                class: "tabs tabs-bordered w-full",
                role: "tablist",

                if has_notification_details_preview {
                    render! {
                        button {
                            class: "tab {notification_tab_style}",
                            role: "tab",
                            onclick: move |_| { ui_model_ref.write().selected_preview_pane = PreviewPane::Notification },
                            div {
                                class: "flex gap-2",
                                NotificationMetadataIcon { class: "h-5 w-5", notification_metadata: &notification.metadata }
                                "Notification"
                            }
                        }
                    }
                }
                if has_task_details_preview {
                    render! {
                        button {
                            class: "tab {task_tab_style}",
                            role: "tab",
                            onclick: move |_| { ui_model_ref.write().selected_preview_pane = PreviewPane::Task },
                            div {
                                class: "flex gap-2",
                                if let Some(task) = &notification.task {
                                    render! {
                                        TaskIcon { class: "h-5 w-5", _kind: task.kind }
                                    }
                                }
                                "Task"
                            }
                        }
                    }
                }
            }

            match ui_model_ref.read().selected_preview_pane {
                PreviewPane::Notification => render! {
                    NotificationDetailsPreview { notification: notification }
                },
                PreviewPane::Task => render! {
                    TaskDetailsPreview { notification: notification }
                },
            }
        }
    }
}

#[component]
fn NotificationDetailsPreview<'a>(cx: Scope, notification: &'a NotificationWithTask) -> Element {
    if let Some(details) = &notification.details {
        return match details {
            NotificationDetails::GithubPullRequest(github_pull_request) => render! {
                GithubPullRequestPreview { github_pull_request: github_pull_request }
            },
            NotificationDetails::GithubDiscussion(github_discussion) => render! {
                GithubDiscussionPreview { github_discussion: github_discussion }
            },
            NotificationDetails::SlackMessage(slack_message) => render! {
                SlackMessagePreview { notification: notification, slack_message: slack_message }
            },
            NotificationDetails::SlackFile(slack_file) => render! {
                SlackFilePreview { notification: notification, slack_file: slack_file }
            },
            NotificationDetails::SlackFileComment(slack_file_comment) => render! {
                SlackFileCommentPreview { notification: notification, slack_file_comment: slack_file_comment }
            },
            NotificationDetails::SlackChannel(slack_channel) => render! {
                SlackChannelPreview { notification: notification, slack_channel: slack_channel }
            },
            NotificationDetails::SlackIm(slack_im) => render! {
                SlackImPreview { notification: notification, slack_im: slack_im }
            },
            NotificationDetails::SlackGroup(slack_group) => render! {
                SlackGroupPreview { notification: notification, slack_group: slack_group }
            },
        };
    }
    match &notification.metadata {
        NotificationMetadata::Github(github_notification) => render! {
            GithubNotificationDefaultPreview {
                notification: notification,
                github_notification: *github_notification.clone(),
            }
        },
        NotificationMetadata::Linear(linear_notification) => render! {
            LinearNotificationPreview { linear_notification: *linear_notification.clone() }
        },
        NotificationMetadata::GoogleMail(google_mail_thread) => render! {
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
fn TaskDetailsPreview<'a>(cx: Scope, notification: &'a NotificationWithTask) -> Element {
    match &notification.task {
        Some(
            task @ Task {
                source_item:
                    ThirdPartyItem {
                        data: ThirdPartyItemData::TodoistItem(todoist_task),
                        ..
                    },
                ..
            },
        ) => render! {
            TodoistTaskPreview {
                notification: notification,
                task: task,
                todoist_task: todoist_task.clone(),
            }
        },
        _ => None,
    }
}
