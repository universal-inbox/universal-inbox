#![allow(non_snake_case)]

use dioxus::prelude::*;
use fermi::UseAtomRef;

use universal_inbox::{
    notification::{
        NotificationDetails, NotificationId, NotificationMetadata, NotificationWithTask,
    },
    task::{Task, TaskMetadata},
};

use crate::{
    components::{
        icons::{GoogleMail, Linear, Todoist},
        integrations::{
            github::{
                icons::Github,
                preview::{
                    discussion::GithubDiscussionPreview, pull_request::GithubPullRequestPreview,
                    GithubNotificationDefaultPreview,
                },
            },
            google_mail::preview::GoogleMailThreadPreview,
            linear::preview::LinearNotificationPreview,
            todoist::preview::TodoistTaskPreview,
        },
    },
    model::{PreviewPane, UniversalInboxUIModel},
};

#[inline_props]
pub fn NotificationPreview<'a>(
    cx: Scope,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
    notification: &'a NotificationWithTask,
) -> Element {
    let is_help_enabled = ui_model_ref.read().is_help_enabled;
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

    let tab_width_style = if has_notification_details_preview && has_task_details_preview {
        "w-1/2"
    } else {
        "w-full"
    };
    let (notification_tab_style, task_tab_style) =
        if ui_model_ref.read().selected_preview_pane == PreviewPane::Notification {
            ("tab-active", "")
        } else {
            ("", "tab-active")
        };
    let (notification_shortcut_visibility_style, task_shortcut_visibility_style) =
        if is_help_enabled && has_notification_details_preview && has_task_details_preview {
            if ui_model_ref.read().selected_preview_pane == PreviewPane::Notification {
                ("invisible", "visible")
            } else {
                ("visible", "invisible")
            }
        } else {
            ("invisible", "invisible")
        };

    render! {
        div {
            class: "flex flex-col gap-4 w-full",

            div {
                class: "tabs w-full",

                if has_notification_details_preview {
                    render! {
                        button {
                            class: "tab tab-bordered {tab_width_style} {notification_tab_style} flex gap-2",
                            onclick: move |_| { ui_model_ref.write().selected_preview_pane = PreviewPane::Notification },
                            NotificationDetailsPreviewIcon { notification: notification }
                            "Notification"
                        }
                    }
                }
                if has_task_details_preview {
                    render! {
                        button {
                            class: "tab tab-bordered {tab_width_style} {task_tab_style} flex gap-2 indicator",
                            onclick: move |_| { ui_model_ref.write().selected_preview_pane = PreviewPane::Task },
                            span {
                                class: "{task_shortcut_visibility_style} indicator-item indicator-top indicator-start badge text-xs text-gray-400 z-50",
                                "▶︎"
                            }
                            span {
                                class: "{notification_shortcut_visibility_style} indicator-item indicator-top indicator-start badge text-xs text-gray-400 z-50",
                                "◀︎"
                            }
                            TaskDetailsPreviewIcon { notification: notification }
                            "Task"
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

#[inline_props]
fn NotificationDetailsPreview<'a>(cx: Scope, notification: &'a NotificationWithTask) -> Element {
    if let Some(details) = &notification.details {
        return match details {
            NotificationDetails::GithubPullRequest(github_pull_request) => render! {
                GithubPullRequestPreview { github_pull_request: github_pull_request }
            },
            NotificationDetails::GithubDiscussion(github_discussion) => render! {
                GithubDiscussionPreview { _github_discussion: github_discussion }
            },
        };
    }
    match &notification.metadata {
        NotificationMetadata::Github(github_notification) => render! {
            GithubNotificationDefaultPreview {
                notification: notification,
                github_notification: github_notification.clone(),
            }
        },
        NotificationMetadata::Linear(linear_notification) => render! {
            LinearNotificationPreview {
                notification: notification,
                linear_notification: linear_notification.clone()
            }
        },
        NotificationMetadata::GoogleMail(google_mail_thread) => render! {
            GoogleMailThreadPreview {
                notification: notification,
                google_mail_thread: google_mail_thread.clone()
            }
        },
        _ => None,
    }
}

#[inline_props]
fn NotificationDetailsPreviewIcon<'a>(
    cx: Scope,
    notification: &'a NotificationWithTask,
) -> Element {
    match &notification.metadata {
        NotificationMetadata::Github(_) => {
            render! { Github { class: "h-5 w-5" } }
        }
        NotificationMetadata::Linear(_) => {
            render! { Linear { class: "h-5 w-5" } }
        }
        NotificationMetadata::GoogleMail(_) => {
            render! { GoogleMail { class: "h-5 w-5" } }
        }
        _ => None,
    }
}

#[inline_props]
fn TaskDetailsPreview<'a>(cx: Scope, notification: &'a NotificationWithTask) -> Element {
    match &notification.task {
        Some(
            task @ Task {
                metadata: TaskMetadata::Todoist(todoist_task),
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

#[inline_props]
fn TaskDetailsPreviewIcon<'a>(cx: Scope, notification: &'a NotificationWithTask) -> Element {
    match &notification.task {
        Some(Task {
            metadata: TaskMetadata::Todoist(_),
            ..
        }) => render! { Todoist { class: "h-5 w-5" } },
        _ => None,
    }
}
