use std::collections::HashSet;

use chrono::{DateTime, Local};
use dioxus::{events::MouseEvent, prelude::*};
use dioxus_free_icons::{
    icons::{
        bs_icons::{
            BsArrowRepeat, BsBellSlash, BsBookmarkCheck, BsCalendar2Check, BsCardChecklist, BsChat,
            BsCheck2, BsCheckCircle, BsClockHistory, BsExclamationCircle, BsGrid, BsLink45deg,
            BsRecordCircle, BsStar, BsTrash,
        },
        io_icons::IoGitPullRequest,
    },
    Icon,
};
use fermi::UseAtomRef;
use http::Uri;

use universal_inbox::{
    notification::{
        integrations::{
            github::GithubNotification,
            google_mail::{
                GoogleMailThread, MessageSelection, GOOGLE_MAIL_IMPORTANT_LABEL,
                GOOGLE_MAIL_STARRED_LABEL,
            },
            linear::{LinearIssue, LinearNotification},
        },
        NotificationMetadata, NotificationWithTask,
    },
    task::{
        integrations::todoist::{TodoistItem, TodoistItemPriority},
        Task, TaskMetadata,
    },
    HasHtmlUrl,
};

use crate::{
    components::icons::{github, google_mail, linear, mail, todoist},
    model::UniversalInboxUIModel,
};

#[inline_props]
pub fn notifications_list<'a>(
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

    cx.render(rsx!(table {
        class: "table w-full h-max-full",

        tbody {
            notifications.iter().enumerate().map(|(i, notif)| {
                let is_selected = i == selected_notification_index;

                rsx!{
                    (!notif.is_built_from_task()).then(|| rsx!(
                        self::notification {
                            notif: notif,
                            selected: is_selected,
                            ui_model_ref: ui_model_ref,

                            self::notification_button {
                                title: "Delete notification",
                                shortcut: "d",
                                selected: is_selected,
                                show_shortcut: is_help_enabled,
                                onclick: |_| on_delete.call(notif),
                                Icon { class: "w-5 h-5", icon: BsTrash }
                            }

                            if notif.task.is_some() {
                                rsx!(
                                    self::notification_button {
                                        title: "Complete task",
                                        shortcut: "c",
                                        selected: is_selected,
                                        disabled_label: is_task_actions_disabled.then_some("No task management service connected"),
                                        show_shortcut: is_help_enabled,
                                        onclick: |_| on_complete_task.call(notif),
                                        Icon { class: "w-5 h-5", icon: BsCheck2 }
                                    }
                                )
                            }

                            self::notification_button {
                                title: "Unsubscribe from the notification",
                                shortcut: "u",
                                selected: is_selected,
                                show_shortcut: is_help_enabled,
                                onclick: |_| on_unsubscribe.call(notif),
                                Icon { class: "w-5 h-5", icon: BsBellSlash }
                            }

                            self::notification_button {
                                title: "Snooze notification",
                                shortcut: "s",
                                selected: is_selected,
                                show_shortcut: is_help_enabled,
                                onclick: |_| on_snooze.call(notif),
                                Icon { class: "w-5 h-5", icon: BsClockHistory }
                            }

                            if notif.task.is_none() {
                                rsx!(
                                    self::notification_button {
                                        title: "Create task",
                                        shortcut: "p",
                                        selected: is_selected,
                                        disabled_label: is_task_actions_disabled.then_some("No task management service connected"),
                                        show_shortcut: is_help_enabled,
                                        onclick: |_| on_plan.call(notif),
                                        Icon { class: "w-5 h-5", icon: BsCalendar2Check }
                                    }

                                    self::notification_button {
                                        title: "Link to task",
                                        shortcut: "a",
                                        selected: is_selected,
                                        disabled_label: is_task_actions_disabled.then_some("No task management service connected"),
                                        show_shortcut: is_help_enabled,
                                        onclick: |_| on_link.call(notif),
                                        Icon { class: "w-5 h-5", icon: BsLink45deg }
                                    }
                                )
                            }
                        }
                    )),

                    (notif.is_built_from_task()).then(|| rsx!(
                        self::notification {
                            notif: notif,
                            selected: is_selected,
                            ui_model_ref: ui_model_ref,

                            self::notification_button {
                                title: "Delete task",
                                shortcut: "d",
                                selected: is_selected,
                                disabled_label: is_task_actions_disabled.then_some("No task management service connected"),
                                show_shortcut: is_help_enabled,
                                onclick: |_| on_delete.call(notif),
                                Icon { class: "w-5 h-5", icon: BsTrash }
                            }

                            self::notification_button {
                                title: "Complete task",
                                shortcut: "c",
                                selected: is_selected,
                                disabled_label: is_task_actions_disabled.then_some("No task management service connected"),
                                show_shortcut: is_help_enabled,
                                onclick: |_| on_complete_task.call(notif),
                                Icon { class: "w-5 h-5", icon: BsCheck2 }
                            }

                            self::notification_button {
                                title: "Snooze notification",
                                shortcut: "s",
                                selected: is_selected,
                                show_shortcut: is_help_enabled,
                                onclick: |_| on_snooze.call(notif),
                                Icon { class: "w-5 h-5", icon: BsClockHistory }
                            }

                            self::notification_button {
                                title: "Plan task",
                                shortcut: "p",
                                selected: is_selected,
                                disabled_label: is_task_actions_disabled.then_some("No task management service connected"),
                                show_shortcut: is_help_enabled,
                                onclick: |_| on_plan.call(notif),
                                Icon { class: "w-5 h-5", icon: BsCalendar2Check }
                            }
                        }
                    ))
                }
            })
        }
    }))
}

#[inline_props]
fn notification<'a>(
    cx: Scope,
    notif: &'a NotificationWithTask,
    selected: bool,
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

    cx.render(rsx!(
        tr {
            class: "hover py-1 {style} group snap-start",
            key: "{notif.id}",
            onmousemove: |_| {
                if ui_model_ref.write_silent().set_unhover_element(false) {
                    cx.needs_update();
                }
            },

            self::notification_display { notif: notif, selected: *selected, children }
        }
    ))
}

#[inline_props]
fn notification_display<'a>(
    cx: Scope,
    notif: &'a NotificationWithTask,
    selected: bool,
    children: Element<'a>,
) -> Element {
    // tag: New notification integration
    let icon = match notif.metadata {
        NotificationMetadata::Github(_) => cx.render(rsx!(self::github { class: "h-5 w-5" })),
        NotificationMetadata::Linear(_) => cx.render(rsx!(self::linear { class: "h-5 w-5" })),
        NotificationMetadata::GoogleMail(_) => {
            cx.render(rsx!(self::google_mail { class: "h-5 w-5" }))
        }
        NotificationMetadata::Todoist => cx.render(rsx!(self::todoist { class: "h-5 w-5" })),
    };
    let button_style = use_memo(cx, (selected,), |(selected,)| {
        if selected {
            "swap-active"
        } else {
            "group-hover:swap-active"
        }
    });
    let notif_updated_at = use_memo(cx, &(notif.updated_at,), |(updated_at,)| {
        Into::<DateTime<Local>>::into(updated_at).format("%Y-%m-%d %H:%M")
    });

    cx.render(rsx!(
        td {
            class: "px-2 py-0 rounded-none relative",
            div { class: "flex justify-center", icon }
            if let Some(ref task) = notif.task {
                rsx!(self::task_hint { task: task })
            }
        }
        td {
            class: "px-2 py-0",

            // tag: New notification integration
            match &notif.metadata {
                NotificationMetadata::Github(github_notification) => rsx!(
                    self::github_notification_display {
                        notif: notif,
                        github_notification: github_notification.clone(),
                    }
                ),
                NotificationMetadata::Linear(linear_notification) => rsx!(
                    self::linear_notification_display {
                        notif: notif,
                        linear_notification: linear_notification.clone()
                    }
                ),
                NotificationMetadata::GoogleMail(google_mail_thread) => rsx!(
                    self::google_mail_thread_display {
                        notif: notif,
                        google_mail_thread: google_mail_thread.clone()
                    }
                ),
                NotificationMetadata::Todoist => rsx!(
                    if let Some(task) = &notif.task {
                        match &task.metadata {
                            TaskMetadata::Todoist(todoist_task) => rsx!(
                                self::todoist_notification_display {
                                    notif: notif,
                                    todoist_task: todoist_task.clone(),
                                }
                            )
                        }
                    } else {
                        rsx!(self::default_notification_display { notif: notif })
                    }
                )
            }
        }
        td {
            class: "px-2 py-0 rounded-none flex flex-wrap items-center justify-end",
            div {
                class: "swap {button_style}",
                div {
                    class: "swap-on flex items-center justify-end",
                    children
                }
                div {
                    class: "swap-off text-xs text-gray-400 flex items-center justify-end group-hover:invisible",
                    "{notif_updated_at}"
                }
            }
        }
    ))
}

#[inline_props]
fn default_notification_display<'a>(cx: Scope, notif: &'a NotificationWithTask) -> Element {
    let link = notif.get_html_url();
    cx.render(rsx!(
        div {
            class: "flex items-center gap-2",

            div { class: "flex flex-col h-5 w-5" }

            div {
                class: "flex flex-col grow",
                a { href: "{link}", target: "_blank", "{notif.title}" }
            }
        }
    ))
}

#[inline_props]
fn github_notification_display<'a>(
    cx: Scope,
    notif: &'a NotificationWithTask,
    github_notification: GithubNotification,
) -> Element {
    let github_notification_id = extract_github_notification_id(&notif.source_html_url);
    let notification_source_url = notif.get_html_url();
    let type_icon = match github_notification.subject.r#type.as_str() {
        "PullRequest" => cx.render(rsx!(Icon {
            class: "h-5 w-5",
            icon: IoGitPullRequest
        })),
        "Issue" => cx.render(rsx!(Icon {
            class: "h-5 w-5",
            icon: BsRecordCircle
        })),
        "Discussion" => cx.render(rsx!(Icon {
            class: "h-5 w-5",
            icon: BsChat
        })),
        "CheckSuite" => cx.render(rsx!(Icon {
            class: "h-5 w-5",
            icon: BsCheckCircle
        })),
        _ => None,
    };

    cx.render(rsx!(
        div {
            class: "flex items-center gap-2",

            type_icon

            div {
                class: "flex flex-col grow",

                a {
                    href: "{notification_source_url}",
                    target: "_blank",
                    "{notif.title}"
                }
                div {
                    class: "flex gap-2",

                    a {
                        class: "text-xs text-gray-400",
                        href: "{github_notification.repository.html_url.clone()}",
                        target: "_blank",
                        "{github_notification.repository.full_name}"
                    }

                    a {
                        class: "text-xs text-gray-400",
                        href: "{notification_source_url}",
                        target: "_blank",
                        if let Some(github_notification_id) = github_notification_id {
                            rsx!("#{github_notification_id} ")
                        }
                        "({github_notification.reason})"
                    }
                }
            }
        }
    ))
}

fn extract_github_notification_id(url: &Option<Uri>) -> Option<String> {
    let Some(url) = url else { return None };
    let mut url_parts = url.path().split('/').collect::<Vec<_>>();
    let id = url_parts.pop()?;
    Some(id.to_string())
}

#[inline_props]
fn linear_notification_display<'a>(
    cx: Scope,
    notif: &'a NotificationWithTask,
    linear_notification: LinearNotification,
) -> Element {
    let notification_source_url = notif.get_html_url();
    let notification_type = linear_notification.get_type();
    let type_icon = match linear_notification {
        LinearNotification::IssueNotification { .. } => cx.render(rsx!(Icon {
            class: "h-5 w-5",
            icon: BsRecordCircle
        })),
        LinearNotification::ProjectNotification { .. } => cx.render(rsx!(Icon {
            class: "h-5 w-5",
            icon: BsGrid
        })),
    };

    cx.render(rsx!(
        div {
            class: "flex items-center gap-2",

            type_icon

            div {
                class: "flex flex-col grow",

                a {
                    href: "{notification_source_url}",
                    target: "_blank",
                    "{notif.title}"
                }
                div {
                    class: "flex gap-2",

                    if let Some(team) = linear_notification.get_team() {
                        rsx!(
                            a {
                                class: "text-xs text-gray-400",
                                href: "{team.get_url(linear_notification.get_organization())}",
                                target: "_blank",
                                "{team.name}"
                            }
                        )
                    }

                    a {
                        class: "text-xs text-gray-400",
                        href: "{notification_source_url}",
                        target: "_blank",
                        if let LinearNotification::IssueNotification {
                            issue: LinearIssue { identifier, .. }, ..
                        } = linear_notification {
                            rsx!("#{identifier} ")
                        }
                        "({notification_type})"
                    }
                }
            }
        }
    ))
}

#[inline_props]
fn google_mail_thread_display<'a>(
    cx: Scope,
    notif: &'a NotificationWithTask,
    google_mail_thread: GoogleMailThread,
) -> Element {
    let notification_source_url = notif.get_html_url();
    let is_starred = google_mail_thread.is_tagged_with(GOOGLE_MAIL_STARRED_LABEL, None);
    let is_important = google_mail_thread.is_tagged_with(GOOGLE_MAIL_IMPORTANT_LABEL, None);
    let from_address = google_mail_thread.get_message_header(MessageSelection::First, "From");
    let interlocutors_count = google_mail_thread
        .messages
        .iter()
        .fold(HashSet::new(), |mut acc, msg| {
            if let Some(from_address) = msg.get_header("From") {
                acc.insert(from_address);
            }
            acc
        })
        .len();
    let mail_icon_style = match (is_starred, is_important) {
        (_, true) => "text-red-500",
        (true, false) => "text-yellow-500",
        _ => "",
    };

    cx.render(rsx!(
        div {
            class: "flex items-center gap-2",

            self::mail { class: "h-5 w-5 {mail_icon_style}" }

            div {
                class: "flex flex-col grow",

                div {
                    class: "flex flex-row items-center",
                    a {
                        class: "mx-0.5",
                        href: "{notification_source_url}",
                        target: "_blank",
                        "{notif.title}"
                    }
                    if is_starred {
                        rsx!(Icon { class: "mx-0.5 h-3 w-3 text-yellow-500", icon: BsStar })
                    }
                    if is_important {
                        rsx!(Icon { class: "mx-0.5 h-3 w-3 text-red-500", icon: BsExclamationCircle })
                    }
                }

                div {
                    class: "flex gap-2",

                    if let Some(from_address) = from_address {
                        rsx!(span { class: "text-xs text-gray-400", "{from_address}" })
                    }
                    span { class: "text-xs text-gray-400", "({interlocutors_count})" }
                }
            }
        }
    ))
}

#[inline_props]
fn todoist_notification_display<'a>(
    cx: Scope,
    notif: &'a NotificationWithTask,
    todoist_task: TodoistItem,
) -> Element {
    let notification_source_url = notif
        .source_html_url
        .as_ref()
        .map(|url| url.to_string())
        .unwrap_or_else(|| "https://todoist.com/app".to_string());
    let task_icon_style = match todoist_task.priority {
        TodoistItemPriority::P1 => "",
        TodoistItemPriority::P2 => "text-yellow-500",
        TodoistItemPriority::P3 => "text-orange-500",
        TodoistItemPriority::P4 => "text-red-500",
    };

    cx.render(rsx!(
        div {
            class: "flex items-center gap-2",

            Icon { class: "h-5 w-5 {task_icon_style}", icon: BsCardChecklist }

            div {
                class: "flex flex-col grow",

                a {
                    href: "{notification_source_url}",
                    target: "_blank",
                    "{notif.title}"
                }
                div {
                    class: "flex gap-2",

                    if let Some(due) = &todoist_task.due {
                        rsx!(div {
                            class: "flex items-center text-xs text-gray-400 gap-1",

                            Icon { class: "h-3 w-3", icon: BsCalendar2Check }
                            "{due.date}"
                            if due.is_recurring {
                                rsx!(Icon { class: "h-3 w-3", icon: BsArrowRepeat })
                            }
                        })
                    }

                    div {
                        class: "flex gap-2",
                        for label in &todoist_task.labels {
                            rsx!(span { class: "text-xs text-gray-400", "@{label}" })
                        }
                    }
                }
            }
        }
    ))
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

fn notification_button<'a>(cx: Scope<'a, NotificationButtonProps<'a>>) -> Element {
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

    cx.render(rsx!(if let Some(Some(label)) = cx.props.disabled_label {
        rsx!(
            div {
                class: "tooltip tooltip-left text-xs text-gray-400",
                "data-tip": "{label}",

                button {
                    class: "btn btn-ghost btn-square btn-disabled",
                    title: "{cx.props.title}",

                    &cx.props.children
                }
            }
        )
    } else {
        rsx!(
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
        )
    }))
}

#[inline_props]
fn task_hint<'a>(cx: Scope, task: &'a Task) -> Element {
    let kind = task.get_task_source_kind();
    let html_url = task.get_html_url();

    cx.render(rsx!(
        div {
            class: "absolute top-0 right-0 tooltip tooltip-right text-xs text-gray-400",
            "data-tip": "Linked to a {kind} task",

            a {
                href: "{html_url}",
                target: "_blank",
                Icon { class: "w-4 h-4", icon: BsBookmarkCheck }
            }
        }
    ))
}
