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
            google_calendar::preview::GoogleCalendarEventPreview,
            google_mail::preview::GoogleMailThreadPreview,
            icons::{NotificationIcon, TaskIcon},
            linear::preview::LinearNotificationPreview,
            slack::preview::{
                channel::SlackChannelPreview, file::SlackFilePreview,
                file_comment::SlackFileCommentPreview, group::SlackGroupPreview,
                im::SlackImPreview, message::SlackMessagePreview, thread::SlackThreadPreview,
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
    let shortcut_visibility_style = use_memo(move || {
        if ui_model.read().is_help_enabled {
            "visible"
        } else {
            "invisible"
        }
    });

    let mut latest_shown_notification_id = use_signal(|| None::<NotificationId>);
    use_effect(move || {
        // reset selected_preview_pane when showing another notification
        let mut latest_shown_notification_id = latest_shown_notification_id.write();
        if *latest_shown_notification_id != Some(notification().id) {
            ui_model.write().selected_preview_pane = if has_notification_details_preview {
                PreviewPane::Notification
            } else {
                PreviewPane::Task
            };
            *latest_shown_notification_id = Some(notification().id);
        }
    });

    let (notification_tab_style, task_tab_style) =
        if ui_model.read().selected_preview_pane == PreviewPane::Notification {
            ("active", "")
        } else {
            ("", "active")
        };

    rsx! {
        div {
            class: "flex flex-col gap-4 w-full",

            div {
                class: "relative w-full",

                span {
                    class: "{shortcut_visibility_style} kbd kbd-xs z-50 absolute left-0",
                    "▼ j"
                }
                span {
                    class: "{shortcut_visibility_style} kbd kbd-xs z-50 absolute right-0",
                    "▲ k"
                }
                nav {
                    class: "tabs tabs-bordered w-full",
                    role: "tablist",

                    if has_notification_details_preview {
                        button {
                            class: "tab active-tab:tab-active {notification_tab_style} w-full",
                            "data-tab": "#notification-tab",
                            role: "tab",
                            onclick: move |_| { ui_model.write().selected_preview_pane = PreviewPane::Notification },
                            div {
                                class: "flex gap-2 items-center text-base-content",
                                NotificationIcon { kind: notification().kind }
                                "Notification"
                            }
                        }
                    }
                    if has_task_details_preview {
                        button {
                            class: "tab active-tab:tab-active {task_tab_style} w-full",
                            "data-tab": "#task-tab",
                            role: "tab",
                            onclick: move |_| { ui_model.write().selected_preview_pane = PreviewPane::Task },
                            div {
                                class: "flex gap-2 text-base-content",
                                if let Some(task) = notification().task {
                                    TaskIcon { class: "h-5 w-5", _kind: task.kind }
                                }
                                "Task"
                            }
                        }
                    }
                }
            }

            if shortcut_visibility_style == "visible" {
                span {
                    class: "{shortcut_visibility_style} kbd kbd-xs z-50",
                    "e: expand/collapse"
                }
                if has_task_details_preview {
                    span {
                        class: "{shortcut_visibility_style} kbd kbd-xs z-50",
                        "tab: switch between tabs"
                    }
                }
            }

            match ui_model.read().selected_preview_pane {
                PreviewPane::Notification => rsx! {
                    div {
                        id: "notification-tab",
                        NotificationDetailsPreview {
                            notification,
                            expand_details: ui_model.read().preview_cards_expanded
                        }
                    }
                },
                PreviewPane::Task => rsx! {
                    if let Some(task) = notification().task {
                        div {
                            id: "task-tab",
                            TaskDetailsPreview {
                                task,
                                expand_details: ui_model.read().preview_cards_expanded
                            }
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn NotificationDetailsPreview(
    notification: ReadOnlySignal<NotificationWithTask>,
    expand_details: ReadOnlySignal<bool>,
) -> Element {
    match notification().source_item.data {
        ThirdPartyItemData::GithubNotification(github_notification) => {
            match github_notification.item {
                Some(GithubNotificationItem::GithubPullRequest(github_pull_request)) => rsx! {
                    GithubPullRequestPreview { github_pull_request, expand_details }
                },
                Some(GithubNotificationItem::GithubDiscussion(github_discussion)) => rsx! {
                    GithubDiscussionPreview { github_discussion, expand_details }
                },
                _ => rsx! {
                    GithubNotificationDefaultPreview {
                        notification,
                        github_notification: *github_notification
                    }
                },
            }
        }
        ThirdPartyItemData::SlackReaction(slack_reaction) => match slack_reaction.item {
            SlackReactionItem::SlackMessage(slack_message) => rsx! {
                SlackMessagePreview { slack_message, title: notification().title }
            },
            SlackReactionItem::SlackFile(slack_file) => rsx! {
                SlackFilePreview { slack_file, title: notification().title }
            },
        },
        ThirdPartyItemData::SlackStar(slack_star) => match slack_star.item {
            SlackStarItem::SlackMessage(slack_message) => rsx! {
                SlackMessagePreview {
                    slack_message: *slack_message,
                    title: notification().title
                }
            },
            SlackStarItem::SlackFile(slack_file) => rsx! {
                SlackFilePreview {
                    slack_file: *slack_file,
                    title: notification().title
                }
            },
            SlackStarItem::SlackFileComment(slack_file_comment) => rsx! {
                SlackFileCommentPreview {
                    slack_file_comment: *slack_file_comment,
                    title: notification().title
                }
            },
            SlackStarItem::SlackChannel(slack_channel) => rsx! {
                SlackChannelPreview {
                    slack_channel: *slack_channel,
                    title: notification().title
                }
            },
            SlackStarItem::SlackIm(slack_im) => rsx! {
                SlackImPreview {
                    slack_im: *slack_im,
                    title: notification().title
                }
            },
            SlackStarItem::SlackGroup(slack_group) => rsx! {
                SlackGroupPreview {
                    slack_group: *slack_group,
                    title: notification().title
                }
            },
        },
        ThirdPartyItemData::SlackThread(slack_thread) => rsx! {
            SlackThreadPreview {
                slack_thread: *slack_thread,
                title: notification().title,
                expand_details
            }
        },
        ThirdPartyItemData::LinearNotification(linear_notification) => rsx! {
            LinearNotificationPreview {
                linear_notification: *linear_notification,
                expand_details
            }
        },
        ThirdPartyItemData::GoogleMailThread(google_mail_thread) => rsx! {
            GoogleMailThreadPreview {
                notification,
                google_mail_thread: *google_mail_thread
            }
        },
        ThirdPartyItemData::GoogleCalendarEvent(google_calendar_event) => rsx! {
            GoogleCalendarEventPreview {
                notification,
                google_calendar_event: *google_calendar_event,
                expand_details
            }
        },
        ThirdPartyItemData::LinearIssue(_) | ThirdPartyItemData::TodoistItem(_) => rsx! {},
    }
}
