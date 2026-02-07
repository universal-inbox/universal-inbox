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
            api::web_page::preview::WebPagePreview,
            github::preview::{
                GithubNotificationDefaultPreview, discussion::GithubDiscussionPreview,
                pull_request::GithubPullRequestPreview,
            },
            google_calendar::preview::GoogleCalendarEventPreview,
            google_drive::preview::GoogleDriveCommentPreview,
            google_mail::preview::GoogleMailThreadPreview,
            icons::{NotificationIcon, TaskIcon},
            linear::preview::LinearNotificationPreview,
            slack::preview::{
                channel::SlackChannelPreview, file::SlackFilePreview,
                file_comment::SlackFileCommentPreview, group::SlackGroupPreview,
                im::SlackImPreview, message::SlackMessagePreview, thread::SlackThreadPreview,
            },
        },
        notifications_list::{NotificationListContext, get_notification_list_item_action_buttons},
        task_preview::TaskDetailsPreview,
    },
    model::{PreviewPane, UniversalInboxUIModel},
    services::notification_service::NotificationCommand,
};

#[component]
pub fn NotificationPreview(
    ui_model: Signal<UniversalInboxUIModel>,
    notification: ReadSignal<NotificationWithTask>,
    notifications_count: ReadSignal<usize>,
) -> Element {
    let notification_service = use_coroutine_handle::<NotificationCommand>();
    let context = use_memo(move || NotificationListContext {
        is_task_actions_enabled: ui_model.read().is_task_actions_enabled,
        notification_service,
    });
    use_context_provider(move || context);
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
        // reset selected_preview_pane and preview_cards_expanded when showing another notification
        let mut latest_shown_notification_id = latest_shown_notification_id.write();
        if *latest_shown_notification_id != Some(notification().id) {
            let mut ui_model = ui_model.write();
            ui_model.selected_preview_pane = if has_notification_details_preview {
                PreviewPane::Notification
            } else {
                PreviewPane::Task
            };
            ui_model.preview_cards_expanded = false;
            *latest_shown_notification_id = Some(notification().id);
        }
    });

    let previous_button_style = if ui_model
        .read()
        .selected_notification_index
        .unwrap_or_default()
        == 0
    {
        "btn-disabled"
    } else {
        ""
    };
    let next_button_style = if ui_model
        .read()
        .selected_notification_index
        .unwrap_or_default()
        == notifications_count() - 1
    {
        "btn-disabled"
    } else {
        ""
    };

    let (notification_tab_style, task_tab_style) =
        if ui_model.read().selected_preview_pane == PreviewPane::Notification {
            ("active", "")
        } else {
            ("", "active")
        };

    rsx! {
        div {
            class: "flex flex-col w-full h-full",

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
                    class: "tabs tabs-bordered w-full pb-2",
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
                                    TaskIcon { class: "h-5 w-5", kind: task.kind }
                                }
                                "Task"
                            }
                        }
                    }
                }
            }

            button {
                class: "btn btn-text absolute left-0 lg:hidden",
                onclick: move |_| ui_model.write().selected_notification_index = None,
                span { class: "icon-[tabler--arrow-left] size-8" }
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
                        class: "flex-1 overflow-hidden",
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
                            class: "flex-1 overflow-hidden",
                            TaskDetailsPreview {
                                task,
                                expand_details: ui_model.read().preview_cards_expanded
                            }
                        }
                    }
                },
            }

            div {
                class: "flex flex-col w-full gap-2 lg:hidden",

                hr { class: "text-gray-200" }
                div {
                    class: "flex w-full justify-center text-sm text-base-content/50",

                    span { "{ui_model.read().selected_notification_index.unwrap_or_default() + 1} of {notifications_count()}" }
                }

                div {
                    class: "flex w-full",
                    button {
                        "type": "button",
                        class: "btn btn-text btn-square btn-lg {previous_button_style}",
                        "aria-label": "Previous notification",
                        onclick: move |_| {
                            let mut model = ui_model.write();
                            model.selected_notification_index = Some(model.selected_notification_index.unwrap_or_default() - 1);
                        },
                        span { class: "icon-[tabler--chevron-left] size-5" }
                    }

                    for btn in get_notification_list_item_action_buttons(
                        notification,
                        false,
                        Some("btn btn-square btn-primary btn-lg".to_string()),
                        Some("flex-1".to_string())) {
                        { btn }
                    }

                    button {
                        "type": "button",
                        class: "btn btn-text btn-square btn-lg {next_button_style}",
                        "aria-label": "Next notification",
                        onclick: move |_| {
                            let mut model = ui_model.write();
                            model.selected_notification_index = Some(model.selected_notification_index.unwrap_or_default() + 1);
                        },
                        span { class: "icon-[tabler--chevron-right] size-5" }
                    }
                }

            }
        }
    }
}

#[component]
fn NotificationDetailsPreview(
    notification: ReadSignal<NotificationWithTask>,
    expand_details: ReadSignal<bool>,
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
                google_mail_thread: *google_mail_thread,
                expand_details,
            }
        },
        ThirdPartyItemData::GoogleCalendarEvent(google_calendar_event) => rsx! {
            GoogleCalendarEventPreview {
                notification,
                google_calendar_event: *google_calendar_event,
                expand_details
            }
        },
        ThirdPartyItemData::GoogleDriveComment(google_drive_comment) => rsx! {
            GoogleDriveCommentPreview {
                notification,
                google_drive_comment: *google_drive_comment,
                expand_details
            }
        },
        ThirdPartyItemData::WebPage(web_page) => rsx! {
            WebPagePreview { notification, web_page: *web_page }
        },
        ThirdPartyItemData::LinearIssue(_) | ThirdPartyItemData::TodoistItem(_) => rsx! {},
    }
}
