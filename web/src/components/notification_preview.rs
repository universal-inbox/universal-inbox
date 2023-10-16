#![allow(non_snake_case)]

use std::collections::HashSet;

use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::{
        bs_icons::{
            BsArrowRepeat, BsArrowUpRightSquare, BsCalendar2Check, BsCardChecklist, BsChat,
            BsCheckCircle, BsFlag, BsGrid, BsRecordCircle,
        },
        io_icons::IoGitPullRequest,
    },
    Icon,
};

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

use crate::components::icons::Mail;

#[inline_props]
pub fn NotificationPreview<'a>(cx: Scope, notification: &'a NotificationWithTask) -> Element {
    match &notification.metadata {
        NotificationMetadata::Github(github_notification) => render! {
            GithubNotificationPreview {
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
        NotificationMetadata::Todoist => {
            if let Some(task) = &notification.task {
                match &task.metadata {
                    TaskMetadata::Todoist(todoist_task) => render! {
                        TodoistTaskPreview {
                            notification: notification,
                            task: task,
                            todoist_task: todoist_task.clone(),
                        }
                    },
                }
            } else {
                render! { DefaultNotificationPreview { notification: notification } }
            }
        }
    }
}

#[inline_props]
fn DefaultNotificationPreview<'a>(cx: Scope, notification: &'a NotificationWithTask) -> Element {
    let link = notification.get_html_url();

    render! {
        div {
            class: "flex flex-row gap-2 w-full divide-y divide-base-200",

            h2 {
                class: "flex items-center gap-2 text-lg",

                a {
                    class: "flex",
                    href: "{link}",
                    target: "_blank",
                    "{notification.title}"
                }
                a {
                    class: "flex-none",
                    href: "{link}",
                    target: "_blank",
                    Icon { class: "h-5 w-5 text-gray-400 p-1", icon: BsArrowUpRightSquare }
                }
            }
        }
    }
}

#[inline_props]
fn GithubNotificationPreview<'a>(
    cx: Scope,
    notification: &'a NotificationWithTask,
    github_notification: GithubNotification,
) -> Element {
    let github_notification_id = github_notification.extract_id();
    let link = notification.get_html_url();
    let type_icon = match github_notification.subject.r#type.as_str() {
        "PullRequest" => render! { Icon { class: "flex-none h-5 w-5", icon: IoGitPullRequest } },
        "Issue" => render! { Icon { class: "flex-none h-5 w-5", icon: BsRecordCircle } },
        "Discussion" => render! { Icon { class: "flex-none h-5 w-5", icon: BsChat } },
        "CheckSuite" => render! { Icon { class: "flex-none h-5 w-5", icon: BsCheckCircle } },
        _ => None,
    };

    render! {
        div {
            class: "flex flex-col gap-2 w-full divide-y divide-base-200",

            div {
                class: "flex gap-2",

                a {
                    class: "text-xs text-gray-400",
                    href: "{github_notification.repository.html_url.clone()}",
                    target: "_blank",
                    "{github_notification.repository.full_name}"
                }

                if let Some(github_notification_id) = github_notification_id {
                    render! {
                        a {
                            class: "text-xs text-gray-400",
                            href: "{link}",
                            target: "_blank",
                            "#{github_notification_id}"
                        }
                    }
                }
            }

            h2 {
                class: "flex items-center gap-2 text-lg",

                type_icon
                a {
                    href: "{link}",
                    target: "_blank",
                    "{notification.title}"
                }
                a {
                    class: "flex-none",
                    href: "{link}",
                    target: "_blank",
                    Icon { class: "h-5 w-5 text-gray-400 p-1", icon: BsArrowUpRightSquare }
                }
            }
        }
    }
}

#[inline_props]
fn LinearNotificationPreview<'a>(
    cx: Scope,
    notification: &'a NotificationWithTask,
    linear_notification: LinearNotification,
) -> Element {
    let link = notification.get_html_url();
    let type_icon = match linear_notification {
        LinearNotification::IssueNotification { .. } => render! {
            Icon { class: "flex-none h-5 w-5", icon: BsRecordCircle }
        },
        LinearNotification::ProjectNotification { .. } => render! {
            Icon { class: "flex-none h-5 w-5", icon: BsGrid }
        },
    };

    render! {
        div {
            class: "flex flex-col gap-2 w-full divide-y divide-base-200",

            div {
                class: "flex gap-2",

                if let Some(team) = linear_notification.get_team() {
                    render! {
                        a {
                            class: "text-xs text-gray-400",
                            href: "{team.get_url(linear_notification.get_organization())}",
                            target: "_blank",
                            "{team.name}"
                        }
                    }
                }

                if let LinearNotification::IssueNotification {
                    issue: LinearIssue { identifier, .. }, ..
                } = linear_notification {
                    render! {
                        a {
                            class: "text-xs text-gray-400",
                            href: "{link}",
                            target: "_blank",
                            "#{identifier} "
                        }
                    }
                }
            }

            h2 {
                class: "flex items-center gap-2 text-lg",

                type_icon
                a {
                    href: "{link}",
                    target: "_blank",
                    "{notification.title}"
                }
                a {
                    class: "flex-none",
                    href: "{link}",
                    target: "_blank",
                    Icon { class: "h-5 w-5 text-gray-400 p-1", icon: BsArrowUpRightSquare }
                }
            }
        }
    }
}

#[inline_props]
fn GoogleMailThreadPreview<'a>(
    cx: Scope,
    notification: &'a NotificationWithTask,
    google_mail_thread: GoogleMailThread,
) -> Element {
    let link = notification.get_html_url();
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
    let is_starred = google_mail_thread.is_tagged_with(GOOGLE_MAIL_STARRED_LABEL, None);
    let is_important = google_mail_thread.is_tagged_with(GOOGLE_MAIL_IMPORTANT_LABEL, None);
    let mail_icon_style = match (is_starred, is_important) {
        (_, true) => "text-red-500",
        (true, false) => "text-yellow-500",
        _ => "",
    };

    render! {
        div {
            class: "flex flex-col gap-2 w-full divide-y divide-base-200",

            div {
                class: "flex gap-2",

                if let Some(from_address) = from_address {
                    render! {
                        span { class: "text-xs text-gray-400", "From: {from_address}" }
                        span { class: "text-xs text-gray-400", "({interlocutors_count})" }
                    }
                }
            }

            h2 {
                class: "flex items-center gap-2 text-lg",

                Mail { class: "flex-none h-5 w-5 {mail_icon_style}" }
                a {
                    href: "{link}",
                    target: "_blank",
                    "{notification.title}"
                }
                a {
                    class: "flex-none",
                    href: "{link}",
                    target: "_blank",
                    Icon { class: "h-5 w-5 text-gray-400 p-1", icon: BsArrowUpRightSquare }
                }
            }
        }
    }
}

#[inline_props]
fn TodoistTaskPreview<'a>(
    cx: Scope,
    notification: &'a NotificationWithTask,
    task: &'a Task,
    todoist_task: TodoistItem,
) -> Element {
    let link = notification.get_html_url();
    let project_link = task.get_html_project_url();
    let title = markdown::to_html(&notification.title);
    let body = markdown::to_html(&task.body);
    let priority: u8 = task.priority.into();
    let task_priority_style = match todoist_task.priority {
        TodoistItemPriority::P1 => "",
        TodoistItemPriority::P2 => "text-yellow-500",
        TodoistItemPriority::P3 => "text-orange-500",
        TodoistItemPriority::P4 => "text-red-500",
    };

    render! {
        div {
            class: "flex flex-col gap-2 w-full divide-y divide-base-200",

            div {
                class: "flex gap-2",

                a {
                    class: "text-xs text-gray-400",
                    href: "{project_link}",
                    target: "_blank",
                    "#{task.project}"
                }
            }

            h2 {
                class: "flex items-center gap-2 text-lg",

                Icon { class: "flex-none h-5 w-5 {task_priority_style}", icon: BsCardChecklist }
                a {
                    href: "{link}",
                    target: "_blank",
                    dangerous_inner_html: "{title}"
                }
                a {
                    class: "flex-none",
                    href: "{link}",
                    target: "_blank",
                    Icon { class: "h-5 w-5 text-gray-400 p-1", icon: BsArrowUpRightSquare }
                }
            }

            table {
                class: "table-auto",
                tbody {
                    if let Some(due) = &todoist_task.due {
                        render! {
                            tr {
                                td {
                                    div {
                                        class: "flex items-center gap-1 text-gray-400",
                                        Icon { class: "h-3 w-3", icon: BsCalendar2Check }
                                        "Due date"
                                    }
                                }
                                td {
                                    div {
                                        class: "flex items-center gap-1",
                                        "{due.date}"
                                        if due.is_recurring {
                                            render! { Icon { class: "h-3 w-3", icon: BsArrowRepeat } }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    tr {
                        td {
                            div {
                                class: "flex items-center gap-1 text-gray-400",
                                Icon { class: "h-3 w-3 {task_priority_style}", icon: BsFlag }
                                "Priority"
                            }
                        }
                        td { "{priority}" }
                    }

                    tr {
                        td {
                            div {
                                class: "flex items-center gap-1 text-gray-400",
                                span { "@" }
                                span { "Labels" }
                            }
                        }
                        td {
                            div {
                                class: "flex items-center gap-1",
                                for label in &todoist_task.labels {
                                    render! { span { "@{label}" } }
                                }
                            }
                        }
                    }
                }
            }

            p {
                class: "w-full prose prose-sm",
                dangerous_inner_html: "{body}"
            }
        }
    }
}
