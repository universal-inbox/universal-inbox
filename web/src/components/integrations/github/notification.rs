#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::notification::{
    integrations::github::GithubNotification, NotificationWithTask,
};

use crate::components::integrations::github::icons::GithubNotificationIcon;

#[inline_props]
pub fn GithubNotificationDisplay<'a>(
    cx: Scope,
    notif: &'a NotificationWithTask,
    github_notification: &'a GithubNotification,
) -> Element {
    let github_notification_id = github_notification.extract_id();
    let title = markdown::to_html(&notif.title);

    render! {
        div {
            class: "flex items-center gap-2",

            GithubNotificationIcon {
                class: "h-5 w-5",
                notif: notif,
                github_notification: github_notification
            }

            div {
                class: "flex flex-col grow",

                span { dangerous_inner_html: "{title}" }
                div {
                    class: "flex gap-2 text-xs text-gray-400",

                    span { "{github_notification.repository.full_name}" }
                    if let Some(github_notification_id) = github_notification_id {
                        render! {
                            span { "#{github_notification_id}" }
                        }
                    }
                }
            }
        }
    }
}
