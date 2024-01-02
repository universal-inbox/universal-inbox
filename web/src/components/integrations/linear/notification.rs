#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::notification::{
    integrations::linear::{LinearIssue, LinearNotification, LinearProject},
    NotificationWithTask,
};

use crate::components::{
    integrations::linear::icons::{LinearIssueIcon, LinearProjectIcon},
    Tag, TagDisplay, UserWithAvatar,
};

#[component]
pub fn LinearNotificationDisplay<'a>(
    cx: Scope,
    notif: &'a NotificationWithTask,
    linear_notification: LinearNotification,
) -> Element {
    let type_icon = match linear_notification {
        LinearNotification::IssueNotification { issue, .. } => render! {
            LinearIssueIcon { class: "h-5 w-5", linear_issue: issue }
        },
        LinearNotification::ProjectNotification { project, .. } => render! {
            LinearProjectIcon { class: "h-5 w-5", linear_project: project }
        },
    };

    render! {
        div {
            class: "flex items-center gap-2",

            type_icon

                div {
                    class: "flex flex-col grow",

                    span { "{notif.title}" }
                    div {
                        class: "flex gap-2",

                        if let Some(team) = linear_notification.get_team() {
                            render! {
                                span { class: "text-xs text-gray-400", "{team.name}" }
                            }
                        }

                        if let LinearNotification::IssueNotification {
                            issue: LinearIssue { identifier, .. }, ..
                        } = linear_notification {
                            render! {
                                span { class: "text-xs text-gray-400", "#{identifier}" }
                            }
                        }
                    }
                }
        }
    }
}

#[component]
pub fn LinearNotificationDetailsDisplay<'a>(
    cx: Scope,
    linear_notification: &'a LinearNotification,
) -> Element {
    match linear_notification {
        LinearNotification::IssueNotification { issue, .. } => render! {
            LinearIssueDetailsDisplay { linear_issue: issue }
        },
        LinearNotification::ProjectNotification { project, .. } => render! {
            LinearProjectDetailsDisplay { linear_project: project }
        },
    }
}

#[component]
pub fn LinearIssueDetailsDisplay<'a>(cx: Scope, linear_issue: &'a LinearIssue) -> Element {
    render! {
        div {
            class: "flex items-center gap-2",

            for tag in linear_issue
                .labels
                .iter()
                .map(|label| Into::<Tag>::into(label.clone())) {
                render! { TagDisplay { tag: tag, class: "text-[10px]" } }
            }

            if let Some(assignee) = &linear_issue.assignee {
                render! {
                    UserWithAvatar { avatar_url: assignee.avatar_url.clone(), initials_from: assignee.name.clone() }
                }
            } else {
                render! {
                    UserWithAvatar { avatar_url: None }
                }
            }
        }
    }
}

#[component]
pub fn LinearProjectDetailsDisplay<'a>(cx: Scope, linear_project: &'a LinearProject) -> Element {
    render! {
        div {
            class: "flex gap-2",

            if let Some(lead) = &linear_project.lead {
                render! {
                    UserWithAvatar { avatar_url: lead.avatar_url.clone(), initials_from: lead.name.clone() }
                }
            }
        }
    }
}
