#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{BsChat, BsCheckCircle, BsRecordCircle},
    Icon,
};

use universal_inbox::notification::{
    integrations::github::GithubNotification, NotificationDetails, NotificationWithTask,
};

use super::icons::GithubPullRequestIcon;

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

            GithubNotificationIcon { class: "h-5 w-5", notif: notif, github_notification: github_notification }

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

#[inline_props]
pub fn GithubNotificationIcon<'a>(
    cx: Scope,
    notif: &'a NotificationWithTask,
    github_notification: &'a GithubNotification,
    class: Option<&'a str>,
) -> Element {
    let class = class.unwrap_or_default();
    if let Some(NotificationDetails::GithubPullRequest(pr)) = &notif.details {
        return render! {
            GithubPullRequestIcon { class: "{class}", github_pull_request: pr }
        };
    }

    match github_notification.subject.r#type.as_str() {
        "PullRequest" => render! { GithubPullRequestIcon { class: "{class}" } },
        "Issue" => render! { Icon { class: "{class}", icon: BsRecordCircle } },
        "Discussion" => render! { Icon { class: "{class}", icon: BsChat } },
        "CheckSuite" => render! { Icon { class: "{class}", icon: BsCheckCircle } },
        _ => None,
    }
}
