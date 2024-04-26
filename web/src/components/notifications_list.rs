#![allow(non_snake_case)]

use chrono::{DateTime, Local};
use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{
        BsBellSlash, BsBookmarkCheck, BsCalendar2Check, BsCheck2, BsClockHistory, BsLink45deg,
        BsTrash,
    },
    Icon,
};

use universal_inbox::{
    notification::{NotificationDetails, NotificationMetadata, NotificationWithTask},
    task::Task,
    third_party::item::ThirdPartyItemData,
    HasHtmlUrl,
};

use crate::{
    components::integrations::{
        github::notification::{
            GithubDiscussionDetailsDisplay, GithubNotificationDisplay,
            GithubPullRequestDetailsDisplay,
        },
        google_mail::notification::{
            GoogleMailNotificationDetailsDisplay, GoogleMailThreadDisplay,
        },
        icons::NotificationMetadataIcon,
        linear::notification::{LinearNotificationDetailsDisplay, LinearNotificationDisplay},
        slack::notification::{
            SlackChannelDetailsDisplay, SlackFileCommentDetailsDisplay, SlackFileDetailsDisplay,
            SlackGroupDetailsDisplay, SlackImDetailsDisplay, SlackMessageDetailsDisplay,
            SlackNotificationDisplay,
        },
        todoist::notification::{TodoistNotificationDetailsDisplay, TodoistNotificationDisplay},
    },
    model::UniversalInboxUIModel,
};

#[component]
pub fn NotificationsList(
    notifications: ReadOnlySignal<Vec<NotificationWithTask>>,
    ui_model: Signal<UniversalInboxUIModel>,
    on_delete: EventHandler<NotificationWithTask>,
    on_unsubscribe: EventHandler<NotificationWithTask>,
    on_snooze: EventHandler<NotificationWithTask>,
    on_complete_task: EventHandler<NotificationWithTask>,
    on_plan: EventHandler<NotificationWithTask>,
    on_link: EventHandler<NotificationWithTask>,
) -> Element {
    let selected_notification_index = ui_model.read().selected_notification_index;
    let is_help_enabled = ui_model.read().is_help_enabled;
    let is_task_actions_disabled = !ui_model.read().is_task_actions_enabled;

    rsx! {
        table {
            class: "table w-full h-max-full",

            tbody {
                for (i, notif) in notifications().into_iter().map(Signal::new).enumerate() {
                    if !notif().is_built_from_task() {
                        Notification {
                            notif: notif(),
                            selected: i == selected_notification_index,
                            show_shortcut: is_help_enabled,
                            notification_index: i,
                            ui_model: ui_model,

                            NotificationButton {
                                title: "Delete notification",
                                shortcut: "d",
                                selected: i == selected_notification_index,
                                show_shortcut: is_help_enabled,
                                onclick: move |_| on_delete.call(notif()),
                                Icon { class: "w-5 h-5", icon: BsTrash }
                            }

                            if notif().task.is_some() {
                                NotificationButton {
                                    title: "Complete task",
                                    shortcut: "c",
                                    selected: i == selected_notification_index,
                                    disabled_label: is_task_actions_disabled.then_some("No task management service connected".to_string()),
                                    show_shortcut: is_help_enabled,
                                    onclick: move |_| on_complete_task.call(notif()),
                                    Icon { class: "w-5 h-5", icon: BsCheck2 }
                                }
                            }

                            NotificationButton {
                                title: "Unsubscribe from the notification",
                                shortcut: "u",
                                selected: i == selected_notification_index,
                                show_shortcut: is_help_enabled,
                                onclick: move |_| on_unsubscribe.call(notif()),
                                Icon { class: "w-5 h-5", icon: BsBellSlash }
                            }

                            NotificationButton {
                                title: "Snooze notification",
                                shortcut: "s",
                                selected: i == selected_notification_index,
                                show_shortcut: is_help_enabled,
                                onclick: move |_| on_snooze.call(notif()),
                                Icon { class: "w-5 h-5", icon: BsClockHistory }
                            }

                            if notif().task.is_none() {
                                NotificationButton {
                                    title: "Create task",
                                    shortcut: "p",
                                    selected: i == selected_notification_index,
                                    disabled_label: is_task_actions_disabled.then_some("No task management service connected".to_string()),
                                    show_shortcut: is_help_enabled,
                                    onclick: move |_| on_plan.call(notif()),
                                    Icon { class: "w-5 h-5", icon: BsCalendar2Check }
                                }

                                NotificationButton {
                                    title: "Link to task",
                                    shortcut: "l",
                                    selected: i == selected_notification_index,
                                    disabled_label: is_task_actions_disabled.then_some("No task management service connected".to_string()),
                                    show_shortcut: is_help_enabled,
                                    onclick: move |_| on_link.call(notif()),
                                    Icon { class: "w-5 h-5", icon: BsLink45deg }
                                }
                            }
                        }
                    }

                    if notif().is_built_from_task() {
                        Notification {
                            notif: notif(),
                            selected: i == selected_notification_index,
                            show_shortcut: is_help_enabled,
                            notification_index: i,
                            ui_model: ui_model,

                            NotificationButton {
                                title: "Delete task",
                                shortcut: "d",
                                selected: i == selected_notification_index,
                                disabled_label: is_task_actions_disabled.then_some("No task management service connected".to_string()),
                                show_shortcut: is_help_enabled,
                                onclick: move |_| on_delete.call(notif()),
                                Icon { class: "w-5 h-5", icon: BsTrash }
                            }

                            NotificationButton {
                                title: "Complete task",
                                shortcut: "c",
                                selected: i == selected_notification_index,
                                disabled_label: is_task_actions_disabled.then_some("No task management service connected".to_string()),
                                show_shortcut: is_help_enabled,
                                onclick: move |_| on_complete_task.call(notif()),
                                Icon { class: "w-5 h-5", icon: BsCheck2 }
                            }

                            NotificationButton {
                                title: "Snooze notification",
                                shortcut: "s",
                                selected: i == selected_notification_index,
                                show_shortcut: is_help_enabled,
                                onclick: move |_| on_snooze.call(notif()),
                                Icon { class: "w-5 h-5", icon: BsClockHistory }
                            }

                            NotificationButton {
                                title: "Plan task",
                                shortcut: "p",
                                selected: i == selected_notification_index,
                                disabled_label: is_task_actions_disabled.then_some("No task management service connected".to_string()),
                                show_shortcut: is_help_enabled,
                                onclick: move |_| on_plan.call(notif()),
                                Icon { class: "w-5 h-5", icon: BsCalendar2Check }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Notification(
    notif: ReadOnlySignal<NotificationWithTask>,
    notification_index: ReadOnlySignal<usize>,
    selected: ReadOnlySignal<bool>,
    show_shortcut: ReadOnlySignal<bool>,
    mut ui_model: Signal<UniversalInboxUIModel>,
    children: Element,
) -> Element {
    let style = use_memo(move || if selected() { "active" } else { "" });

    rsx! {
        tr {
            class: "hover flex items-center py-1 {style} group snap-start cursor-pointer",
            key: "{notif().id}",
            onmousemove: move |_| {
                if ui_model.peek().unhover_element {
                    ui_model.write().set_unhover_element(false);
                }
            },
            onclick: move |_| {
                if !selected() {
                    ui_model.write().selected_notification_index = notification_index();
                }
            },

            NotificationDisplay { notif: notif, selected: selected, show_shortcut: show_shortcut, children }
        }
    }
}

#[component]
fn NotificationDisplay(
    notif: ReadOnlySignal<NotificationWithTask>,
    selected: ReadOnlySignal<bool>,
    show_shortcut: ReadOnlySignal<bool>,
    children: Element,
) -> Element {
    let shortcut_visibility_style = if selected() && show_shortcut() {
        "visible"
    } else {
        "invisible"
    };
    // tag: New notification integration
    let notification_display = match notif().metadata {
        NotificationMetadata::Github(github_notification) => rsx! {
            GithubNotificationDisplay {
                notif: notif,
                github_notification: *github_notification,
            }
        },
        NotificationMetadata::Linear(linear_notification) => rsx! {
            LinearNotificationDisplay {
                notif: notif,
                linear_notification: *linear_notification
            }
        },
        NotificationMetadata::GoogleMail(google_mail_thread) => rsx! {
            GoogleMailThreadDisplay {
                notif: notif,
                google_mail_thread: *google_mail_thread
            }
        },
        NotificationMetadata::Slack(slack_push_event_callback) => rsx! {
            SlackNotificationDisplay {
                notif: notif,
                slack_push_event_callback: *slack_push_event_callback
            },
        },
        NotificationMetadata::Todoist => {
            if let Some(task) = notif().task {
                match &task.source_item.data {
                    ThirdPartyItemData::TodoistItem(todoist_task) => rsx! {
                        TodoistNotificationDisplay {
                            notif: notif,
                            todoist_task: todoist_task.clone(),
                        }
                    },
                    _ => rsx! { DefaultNotificationDisplay { notif: notif } },
                }
            } else {
                rsx! { DefaultNotificationDisplay { notif: notif } }
            }
        }
    };

    let (button_active_style, details_style, button_style) = use_memo(move || {
        if selected() {
            ("swap-active", "invisible", "")
        } else {
            ("", "", "invisible")
        }
    })();
    let notif_updated_at = use_memo(move || {
        Into::<DateTime<Local>>::into(notif().updated_at)
            .format("%Y-%m-%d %H:%M")
            .to_string()
    });

    rsx! {
        td {
            class: "flex items-center px-2 py-0 rounded-none relative h-12 indicator",
            span {
                class: "{shortcut_visibility_style} indicator-item indicator-top indicator-start badge text-xs text-gray-400 z-50",
                "▲"
            }
            span {
                class: "{shortcut_visibility_style} indicator-item indicator-bottom indicator-start badge text-xs text-gray-400 z-50",
                "▼"
            }

            div {
                class: "flex justify-center",
                NotificationMetadataIcon { class: "h-5 w-5", notification_metadata: notif().metadata}
            }
            if let Some(task) = notif().task {
                TaskHint { task: task }
            }
        }
        td {
            class: "px-2 py-0 grow",

            { notification_display }
        }
        td {
            class: "px-2 py-0 rounded-none flex items-center justify-end",
            div {
                class: "swap {button_active_style}",
                div {
                    class: "swap-on flex items-center justify-end {button_style}",
                    { children }
                }
                div {
                    class: "swap-off text-xs flex gap-2 items-center justify-end {details_style}",

                    NotificationDetailsDisplay { notification: notif }
                    span { class: "text-gray-400 whitespace-nowrap text-xs font-mono", "{notif_updated_at}" }
                }
            }
        }
    }
}

#[component]
fn DefaultNotificationDisplay(notif: ReadOnlySignal<NotificationWithTask>) -> Element {
    rsx! {
        div {
            class: "flex items-center gap-2",

            div { class: "flex flex-col h-5 w-5 min-w-5" }

            div {
                class: "flex flex-col grow",
                span { "{notif().title}" }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct NotificationButtonProps {
    children: Element,
    title: ReadOnlySignal<String>,
    shortcut: ReadOnlySignal<String>,
    selected: ReadOnlySignal<bool>,
    disabled_label: Option<Option<String>>,
    show_shortcut: ReadOnlySignal<bool>,
    #[props(optional)]
    onclick: Option<EventHandler<MouseEvent>>,
}

fn NotificationButton(props: NotificationButtonProps) -> Element {
    let shortcut_visibility_style = use_memo(move || {
        if *(props.selected.read()) {
            if *(props.show_shortcut.read()) {
                "visible"
            } else {
                "invisible group-hover/notification-button:visible"
            }
        } else {
            "invisible"
        }
    });

    if let Some(Some(label)) = props.disabled_label {
        rsx! {
            div {
                class: "tooltip tooltip-left text-xs text-gray-400",
                "data-tip": "{label}",

                button {
                    class: "btn btn-ghost btn-square btn-disabled",
                    title: "{props.title}",

                    { props.children }
                }
            }
        }
    } else {
        rsx! {
            div {
                class: "indicator group/notification-button",

                span {
                    class: "{shortcut_visibility_style} indicator-item indicator-bottom indicator-center badge text-xs text-gray-400 z-50",
                    "{props.shortcut}"
                }

                button {
                    class: "btn btn-ghost btn-square",
                    title: "{props.title}",
                    onclick: move |evt| {
                        if let Some(handler) = &props.onclick {
                            handler.call(evt)
                        }
                    },

                    { props.children }
                }
            }
        }
    }
}

#[component]
fn TaskHint(task: ReadOnlySignal<Task>) -> Element {
    let html_url = task().get_html_url();

    rsx! {
        div {
            class: "absolute top-0 right-0 tooltip tooltip-right text-xs text-gray-400",
           "data-tip": "Linked to a {task().kind} task",

            a {
                href: "{html_url}",
                target: "_blank",
                Icon { class: "w-4 h-4", icon: BsBookmarkCheck }
            }
        }
    }
}

#[component]
pub fn NotificationDetailsDisplay(notification: ReadOnlySignal<NotificationWithTask>) -> Element {
    if let Some(details) = notification().details {
        return match details {
            NotificationDetails::GithubPullRequest(github_pull_request) => rsx! {
                GithubPullRequestDetailsDisplay { github_pull_request: github_pull_request }
            },
            NotificationDetails::GithubDiscussion(github_discussion) => rsx! {
                GithubDiscussionDetailsDisplay { github_discussion: github_discussion }
            },
            NotificationDetails::SlackMessage(slack_message) => rsx! {
                SlackMessageDetailsDisplay { slack_message: slack_message }
            },
            NotificationDetails::SlackChannel(slack_channel) => rsx! {
                SlackChannelDetailsDisplay { slack_channel: slack_channel }
            },
            NotificationDetails::SlackFile(slack_file) => rsx! {
                SlackFileDetailsDisplay { slack_file: slack_file }
            },
            NotificationDetails::SlackFileComment(slack_file_comment) => rsx! {
                SlackFileCommentDetailsDisplay { slack_file_comment: slack_file_comment }
            },
            NotificationDetails::SlackIm(slack_im) => rsx! {
                SlackImDetailsDisplay { slack_im: slack_im }
            },
            NotificationDetails::SlackGroup(slack_group) => rsx! {
                SlackGroupDetailsDisplay { slack_group: slack_group }
            },
        };
    }
    match notification().metadata {
        NotificationMetadata::Linear(linear_notification) => rsx! {
            LinearNotificationDetailsDisplay { linear_notification: *linear_notification }
        },
        NotificationMetadata::Todoist => {
            if let Some(task) = notification().task {
                match &task.source_item.data {
                    ThirdPartyItemData::TodoistItem(todoist_item) => rsx! {
                        TodoistNotificationDetailsDisplay { todoist_item: todoist_item.clone() }
                    },
                    _ => None,
                }
            } else {
                None
            }
        }
        NotificationMetadata::GoogleMail(google_mail_thread) => rsx! {
            GoogleMailNotificationDetailsDisplay { google_mail_thread: *google_mail_thread }
        },
        NotificationMetadata::Github(_) => None,
        NotificationMetadata::Slack(_) => None,
    }
}
