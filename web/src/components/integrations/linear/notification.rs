#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::notification::{
    integrations::linear::{LinearIssue, LinearNotification},
    NotificationWithTask,
};

use crate::components::integrations::linear::icons::{LinearIssueIcon, LinearProjectIcon};

#[inline_props]
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
