#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{
    notification::{integrations::linear::LinearNotification, NotificationWithTask},
    third_party::integrations::linear::{LinearIssue, LinearProject},
};

use crate::components::{
    integrations::linear::{
        get_notification_type_label,
        icons::{LinearIssueIcon, LinearProjectDefaultIcon, LinearProjectIcon},
    },
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
            LinearIssueIcon { class: "h-5 w-5 min-w-5", linear_issue: issue }
        },
        LinearNotification::ProjectNotification { project, .. } => render! {
            LinearProjectIcon { class: "h-5 w-5 min-w-5", linear_project: project }
        },
    };

    render! {
        div {
            class: "flex items-center gap-2",

            type_icon

                div {
                    class: "flex flex-col grow",

                    if let LinearNotification::ProjectNotification {
                        project: LinearProject { icon: Some(project_icon), .. }, ..
                    } = linear_notification {
                        render! { span { "{project_icon} {notif.title}" } }
                    } else {
                        render! {
                            span { "{notif.title}" }
                        }
                    }

                    div {
                        class: "flex gap-2",

                        if let Some(team) = linear_notification.get_team() {
                            if let Some(team_icon) = team.icon {
                                render! {
                                    span { class: "text-xs text-gray-400", "{team_icon} {team.name}" }
                                }
                            } else {
                                render! {
                                    span { class: "text-xs text-gray-400", "{team.name}" }
                                }
                            }
                        }

                        if let LinearNotification::IssueNotification {
                            issue: LinearIssue { identifier, project, .. }, ..
                        } = linear_notification {
                            if let Some(LinearProject { name, icon, .. }) = project {
                                render! {
                                    div {
                                        class: "flex flex-row items-center gap-1 text-xs text-gray-400",
                                        if let Some(project_icon) = icon {
                                            render! { span { "{project_icon}" } }
                                        } else {
                                            render! { LinearProjectDefaultIcon { class: "w-3 h-3" } }
                                        }
                                        "{name} #{identifier}"
                                    }
                                }
                            } else {
                                render! {
                                    span { class: "text-xs text-gray-400", "#{identifier}" }
                                }
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
        LinearNotification::IssueNotification { issue, r#type, .. } => render! {
            LinearIssueDetailsDisplay { notification_type: r#type.clone(),  linear_issue: issue }
        },
        LinearNotification::ProjectNotification {
            project, r#type, ..
        } => render! {
            LinearProjectDetailsDisplay { notification_type: r#type.clone(),  linear_project: project }
        },
    }
}

#[component]
pub fn LinearIssueDetailsDisplay<'a>(
    cx: Scope,
    notification_type: String,
    linear_issue: &'a LinearIssue,
) -> Element {
    render! {
        div {
            class: "flex items-center gap-2",

            div {
                class: "flex flex-wrap items-center gap-1",
                TagDisplay {
                    tag: Into::<Tag>::into(get_notification_type_label(notification_type))
                }
            }

            if let Some(assignee) = &linear_issue.assignee {
                render! {
                    UserWithAvatar { avatar_url: assignee.avatar_url.clone(), user_name: assignee.name.clone() }
                }
            } else {
                render! {
                    UserWithAvatar {}
                }
            }
        }
    }
}

#[component]
pub fn LinearProjectDetailsDisplay<'a>(
    cx: Scope,
    notification_type: String,
    linear_project: &'a LinearProject,
) -> Element {
    render! {
        div {
            class: "flex items-center gap-2",

            div {
                class: "flex flex-wrap items-center gap-1",
                TagDisplay {
                    tag: Into::<Tag>::into(get_notification_type_label(notification_type))
                }
            }

            if let Some(lead) = &linear_project.lead {
                render! {
                    UserWithAvatar { avatar_url: lead.avatar_url.clone(), user_name: lead.name.clone() }
                }
            }
        }
    }
}
