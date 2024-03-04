#![allow(non_snake_case)]

use chrono::{DateTime, Local};
use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{
        BsBellSlash, BsBookmarkCheck, BsCalendar2Check, BsCheck2, BsClockHistory, BsLink45deg,
        BsSlack, BsTrash,
    },
    Icon,
};
use fermi::UseAtomRef;

use universal_inbox::{
    notification::{NotificationDetails, NotificationMetadata, NotificationWithTask},
    task::{Task, TaskMetadata},
    HasHtmlUrl,
};

use crate::{
    components::{
        icons::{GoogleMail, Linear, Todoist},
        integrations::{
            github::{
                icons::Github,
                notification::{
                    GithubDiscussionDetailsDisplay, GithubNotificationDisplay,
                    GithubPullRequestDetailsDisplay,
                },
            },
            google_mail::notification::{
                GoogleMailNotificationDetailsDisplay, GoogleMailThreadDisplay,
            },
            linear::notification::{LinearNotificationDetailsDisplay, LinearNotificationDisplay},
            slack::notification::{
                SlackChannelDetailsDisplay, SlackEventDetailsDisplay,
                SlackFileCommentDetailsDisplay, SlackFileDetailsDisplay, SlackGroupDetailsDisplay,
                SlackImDetailsDisplay, SlackMessageDetailsDisplay, SlackNotificationDisplay,
            },
            todoist::notification::{
                TodoistNotificationDetailsDisplay, TodoistNotificationDisplay,
            },
        },
    },
    model::UniversalInboxUIModel,
};

#[component]
pub fn NotificationsList<'a>(
    cx: Scope,
    notifications: Vec<NotificationWithTask>,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
    on_delete: EventHandler<'a, &'a NotificationWithTask>,
    on_unsubscribe: EventHandler<'a, &'a NotificationWithTask>,
    on_snooze: EventHandler<'a, &'a NotificationWithTask>,
    on_complete_task: EventHandler<'a, &'a NotificationWithTask>,
    on_plan: EventHandler<'a, &'a NotificationWithTask>,
    on_link: EventHandler<'a, &'a NotificationWithTask>,
) -> Element {
    let selected_notification_index = ui_model_ref.read().selected_notification_index;
    let is_help_enabled = ui_model_ref.read().is_help_enabled;
    let is_task_actions_disabled = !ui_model_ref.read().is_task_actions_enabled;

    render! { table {
        class: "table w-full h-max-full",

        tbody {
            notifications.iter().enumerate().map(|(i, notif)| {
                let is_selected = i == selected_notification_index;

                render! {
                    (!notif.is_built_from_task()).then(|| render! {
                        Notification {
                            notif: notif,
                            selected: is_selected,
                            show_shortcut: is_help_enabled,
                            notification_index: i,
                            ui_model_ref: ui_model_ref,

                            NotificationButton {
                                title: "Delete notification",
                                shortcut: "d",
                                selected: is_selected,
                                show_shortcut: is_help_enabled,
                                onclick: |_| on_delete.call(notif),
                                Icon { class: "w-5 h-5", icon: BsTrash }
                            }

                            if notif.task.is_some() {
                                render! {
                                    NotificationButton {
                                        title: "Complete task",
                                        shortcut: "c",
                                        selected: is_selected,
                                        disabled_label: is_task_actions_disabled.then_some("No task management service connected"),
                                        show_shortcut: is_help_enabled,
                                        onclick: |_| on_complete_task.call(notif),
                                        Icon { class: "w-5 h-5", icon: BsCheck2 }
                                    }
                                }
                            }

                            NotificationButton {
                                title: "Unsubscribe from the notification",
                                shortcut: "u",
                                selected: is_selected,
                                show_shortcut: is_help_enabled,
                                onclick: |_| on_unsubscribe.call(notif),
                                Icon { class: "w-5 h-5", icon: BsBellSlash }
                            }

                            NotificationButton {
                                title: "Snooze notification",
                                shortcut: "s",
                                selected: is_selected,
                                show_shortcut: is_help_enabled,
                                onclick: |_| on_snooze.call(notif),
                                Icon { class: "w-5 h-5", icon: BsClockHistory }
                            }

                            if notif.task.is_none() {
                                render! {
                                    NotificationButton {
                                        title: "Create task",
                                        shortcut: "p",
                                        selected: is_selected,
                                        disabled_label: is_task_actions_disabled.then_some("No task management service connected"),
                                        show_shortcut: is_help_enabled,
                                        onclick: |_| on_plan.call(notif),
                                        Icon { class: "w-5 h-5", icon: BsCalendar2Check }
                                    }

                                    NotificationButton {
                                        title: "Link to task",
                                        shortcut: "l",
                                        selected: is_selected,
                                        disabled_label: is_task_actions_disabled.then_some("No task management service connected"),
                                        show_shortcut: is_help_enabled,
                                        onclick: |_| on_link.call(notif),
                                        Icon { class: "w-5 h-5", icon: BsLink45deg }
                                    }
                                }
                            }
                        }
                    }),

                    (notif.is_built_from_task()).then(|| render! {
                        Notification {
                            notif: notif,
                            selected: is_selected,
                            show_shortcut: is_help_enabled,
                            notification_index: i,
                            ui_model_ref: ui_model_ref,

                            NotificationButton {
                                title: "Delete task",
                                shortcut: "d",
                                selected: is_selected,
                                disabled_label: is_task_actions_disabled.then_some("No task management service connected"),
                                show_shortcut: is_help_enabled,
                                onclick: |_| on_delete.call(notif),
                                Icon { class: "w-5 h-5", icon: BsTrash }
                            }

                            NotificationButton {
                                title: "Complete task",
                                shortcut: "c",
                                selected: is_selected,
                                disabled_label: is_task_actions_disabled.then_some("No task management service connected"),
                                show_shortcut: is_help_enabled,
                                onclick: |_| on_complete_task.call(notif),
                                Icon { class: "w-5 h-5", icon: BsCheck2 }
                            }

                            NotificationButton {
                                title: "Snooze notification",
                                shortcut: "s",
                                selected: is_selected,
                                show_shortcut: is_help_enabled,
                                onclick: |_| on_snooze.call(notif),
                                Icon { class: "w-5 h-5", icon: BsClockHistory }
                            }

                            NotificationButton {
                                title: "Plan task",
                                shortcut: "p",
                                selected: is_selected,
                                disabled_label: is_task_actions_disabled.then_some("No task management service connected"),
                                show_shortcut: is_help_enabled,
                                onclick: |_| on_plan.call(notif),
                                Icon { class: "w-5 h-5", icon: BsCalendar2Check }
                            }
                        }
                    })
                }
            })
        }
    } }
}

#[component]
fn Notification<'a>(
    cx: Scope,
    notif: &'a NotificationWithTask,
    notification_index: usize,
    selected: bool,
    show_shortcut: bool,
    ui_model_ref: &'a UseAtomRef<UniversalInboxUIModel>,
    children: Element<'a>,
) -> Element {
    let style = use_memo(
        cx,
        (selected,),
        |(selected,)| {
            if selected {
                "active"
            } else {
                ""
            }
        },
    );

    render! {
        tr {
            class: "hover flex items-center py-1 {style} group snap-start cursor-pointer",
            key: "{notif.id}",
            onmousemove: |_| {
                if ui_model_ref.write_silent().set_unhover_element(false) {
                    cx.needs_update();
                }
            },
            onclick: move |_| {
                if !selected {
                    ui_model_ref.write().selected_notification_index = *notification_index;
                }
            },

            NotificationDisplay { notif: notif, selected: *selected, show_shortcut: *show_shortcut, children }
        }
    }
}

#[component]
fn NotificationDisplay<'a>(
    cx: Scope,
    notif: &'a NotificationWithTask,
    selected: bool,
    show_shortcut: bool,
    children: Element<'a>,
) -> Element {
    let shortcut_visibility_style = if *selected && *show_shortcut {
        "visible"
    } else {
        "invisible"
    };
    // tag: New notification integration
    let icon = match notif.metadata {
        NotificationMetadata::Github(_) => render! { Github { class: "h-5 w-5" } },
        NotificationMetadata::Linear(_) => render! { Linear { class: "h-5 w-5" } },
        NotificationMetadata::GoogleMail(_) => render! { GoogleMail { class: "h-5 w-5" } },
        NotificationMetadata::Todoist => render! { Todoist { class: "h-5 w-5" } },
        NotificationMetadata::Slack(_) => render! { Icon { class: "h-5 w-5", icon: BsSlack } },
    };
    // tag: New notification integration
    let notification_display = match &notif.metadata {
        NotificationMetadata::Github(github_notification) => render! {
            GithubNotificationDisplay {
                notif: notif,
                github_notification: github_notification,
            }
        },
        NotificationMetadata::Linear(linear_notification) => render! {
            LinearNotificationDisplay {
                notif: notif,
                linear_notification: *linear_notification.clone()
            }
        },
        NotificationMetadata::GoogleMail(google_mail_thread) => render! {
            GoogleMailThreadDisplay {
                notif: notif,
                google_mail_thread: *google_mail_thread.clone()
            }
        },
        NotificationMetadata::Slack(slack_push_event_callback) => render! {
            SlackNotificationDisplay {
                notif: notif,
                slack_push_event_callback: *slack_push_event_callback.clone()
            },
        },
        NotificationMetadata::Todoist => {
            if let Some(task) = &notif.task {
                match &task.metadata {
                    TaskMetadata::Todoist(todoist_task) => render! {
                        TodoistNotificationDisplay {
                            notif: notif,
                            todoist_task: todoist_task.clone(),
                        }
                    },
                }
            } else {
                render! { DefaultNotificationDisplay { notif: notif } }
            }
        }
    };

    let (button_active_style, details_style, button_style) =
        use_memo(cx, (selected,), |(selected,)| {
            if selected {
                ("swap-active", "invisible", "")
            } else {
                ("", "", "invisible")
            }
        });
    let notif_updated_at = use_memo(cx, &(notif.updated_at,), |(updated_at,)| {
        Into::<DateTime<Local>>::into(updated_at).format("%Y-%m-%d %H:%M")
    });

    render! {
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

            div { class: "flex justify-center", icon }
            if let Some(ref task) = notif.task {
                render! { TaskHint { task: task } }
            }
        }
        td {
            class: "px-2 py-0 grow",

            notification_display
        }
        td {
            class: "px-2 py-0 rounded-none flex items-center justify-end",
            div {
                class: "swap {button_active_style}",
                div {
                    class: "swap-on flex items-center justify-end {button_style}",
                    children
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
fn DefaultNotificationDisplay<'a>(cx: Scope, notif: &'a NotificationWithTask) -> Element {
    render! {
        div {
            class: "flex items-center gap-2",

            div { class: "flex flex-col h-5 w-5 min-w-5" }

            div {
                class: "flex flex-col grow",
                span { "{notif.title}" }
            }
        }
    }
}

#[derive(Props)]
struct NotificationButtonProps<'a> {
    children: Element<'a>,
    title: &'a str,
    shortcut: &'a str,
    selected: bool,
    disabled_label: Option<Option<&'a str>>,
    show_shortcut: bool,
    #[props(optional)]
    onclick: Option<EventHandler<'a, MouseEvent>>,
}

fn NotificationButton<'a>(cx: Scope<'a, NotificationButtonProps<'a>>) -> Element {
    let shortcut_visibility_style = use_memo(
        cx,
        &(cx.props.selected, cx.props.show_shortcut),
        |(selected, show_shortcut)| {
            if selected {
                if show_shortcut {
                    "visible"
                } else {
                    "invisible group-hover/notification-button:visible"
                }
            } else {
                "invisible"
            }
        },
    );

    if let Some(Some(label)) = cx.props.disabled_label {
        render! {
            div {
                class: "tooltip tooltip-left text-xs text-gray-400",
                "data-tip": "{label}",

                button {
                    class: "btn btn-ghost btn-square btn-disabled",
                    title: "{cx.props.title}",

                    &cx.props.children
                }
            }
        }
    } else {
        render! {
            div {
                class: "indicator group/notification-button",

                span {
                    class: "{shortcut_visibility_style} indicator-item indicator-bottom indicator-center badge text-xs text-gray-400 z-50",
                    "{cx.props.shortcut}"
                }

                button {
                    class: "btn btn-ghost btn-square",
                    title: "{cx.props.title}",
                    onclick: move |evt| {
                        if let Some(handler) = &cx.props.onclick {
                            handler.call(evt)
                        }
                    },

                    &cx.props.children
                }
            }
        }
    }
}

#[component]
fn TaskHint<'a>(cx: Scope, task: &'a Task) -> Element {
    let kind = task.get_task_source_kind();
    let html_url = task.get_html_url();

    render! {
        div {
            class: "absolute top-0 right-0 tooltip tooltip-right text-xs text-gray-400",
           "data-tip": "Linked to a {kind} task",

            a {
                href: "{html_url}",
                target: "_blank",
                Icon { class: "w-4 h-4", icon: BsBookmarkCheck }
            }
        }
    }
}

#[component]
pub fn NotificationDetailsDisplay<'a>(
    cx: Scope,
    notification: &'a NotificationWithTask,
) -> Element {
    if let Some(details) = &notification.details {
        return match details {
            NotificationDetails::GithubPullRequest(github_pull_request) => render! {
                GithubPullRequestDetailsDisplay { github_pull_request: github_pull_request }
            },
            NotificationDetails::GithubDiscussion(github_discussion) => render! {
                GithubDiscussionDetailsDisplay { github_discussion: github_discussion }
            },
            NotificationDetails::SlackMessage(slack_message) => render! {
                SlackMessageDetailsDisplay { _slack_message: slack_message }
            },
            NotificationDetails::SlackChannel(slack_channel) => render! {
                SlackChannelDetailsDisplay { _slack_channel: slack_channel }
            },
            NotificationDetails::SlackFile(slack_file) => render! {
                SlackFileDetailsDisplay { _slack_file: slack_file }
            },
            NotificationDetails::SlackFileComment(slack_file_comment) => render! {
                SlackFileCommentDetailsDisplay { _slack_file_comment: slack_file_comment }
            },
            NotificationDetails::SlackIm(slack_im) => render! {
                SlackImDetailsDisplay { _slack_im: slack_im }
            },
            NotificationDetails::SlackGroup(slack_group) => render! {
                SlackGroupDetailsDisplay { _slack_group: slack_group }
            },
        };
    }
    match &notification.metadata {
        NotificationMetadata::Linear(linear_notification) => render! {
            LinearNotificationDetailsDisplay { linear_notification: linear_notification }
        },
        NotificationMetadata::Todoist => {
            if let Some(task) = &notification.task {
                match &task.metadata {
                    TaskMetadata::Todoist(todoist_item) => render! {
                        TodoistNotificationDetailsDisplay { todoist_item: todoist_item }
                    },
                }
            } else {
                None
            }
        }
        NotificationMetadata::GoogleMail(google_mail_thread) => render! {
            GoogleMailNotificationDetailsDisplay { google_mail_thread: google_mail_thread }
        },
        NotificationMetadata::Github(_) => None,
        NotificationMetadata::Slack(slack_push_event_callback) => render! {
            SlackEventDetailsDisplay { slack_push_event_callback: slack_push_event_callback } // TODO: remove
        },
    }
}
