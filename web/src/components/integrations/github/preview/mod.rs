#![allow(non_snake_case)]

use dioxus::prelude::*;

use dioxus_free_icons::{
    icons::bs_icons::{BsArrowUpRightSquare, BsCheckCircle, BsRecordCircle},
    Icon,
};

use universal_inbox::{
    notification::{integrations::github::GithubNotification, NotificationWithTask},
    HasHtmlUrl,
};

use crate::components::integrations::github::icons::{GithubDiscussionIcon, GithubPullRequestIcon};

pub mod discussion;
pub mod pull_request;

#[inline_props]
pub fn GithubNotificationDefaultPreview<'a>(
    cx: Scope,
    notification: &'a NotificationWithTask,
    github_notification: GithubNotification,
) -> Element {
    let github_notification_id = github_notification.extract_id();
    let link = notification.get_html_url();
    let type_icon = match github_notification.subject.r#type.as_str() {
        "PullRequest" => render! { GithubPullRequestIcon { class: "flex-none h-5 w-5" } },
        "Issue" => render! { Icon { class: "flex-none h-5 w-5", icon: BsRecordCircle } },
        "Discussion" => render! { GithubDiscussionIcon { class: "flex-none h-5 w-5" } },
        "CheckSuite" => render! { Icon { class: "flex-none h-5 w-5", icon: BsCheckCircle } },
        _ => None,
    };

    render! {
        div {
            class: "flex flex-col w-full gap-2",

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
