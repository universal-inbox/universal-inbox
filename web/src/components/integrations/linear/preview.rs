#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{BsArrowUpRightSquare, BsGrid, BsRecordCircle},
    Icon,
};
use universal_inbox::{
    notification::{
        integrations::linear::{LinearIssue, LinearNotification},
        NotificationWithTask,
    },
    HasHtmlUrl,
};

#[inline_props]
pub fn LinearNotificationPreview<'a>(
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
            class: "flex flex-col gap-2 w-full",

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
