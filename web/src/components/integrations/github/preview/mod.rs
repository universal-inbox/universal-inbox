#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{
    HasHtmlUrl, notification::NotificationWithTask,
    third_party::integrations::github::GithubNotification,
};

use crate::components::{
    preview_card_header::PreviewCardHeader,
    ui::{Card, CardVariant, MetadataGrid, MetadataItem},
};

pub mod discussion;
pub mod pull_request;

#[component]
pub fn GithubNotificationDefaultPreview(
    notification: ReadSignal<NotificationWithTask>,
    github_notification: GithubNotification,
) -> Element {
    let github_notification_id = github_notification.extract_id();
    let link = notification().get_html_url();
    let kind_label = match github_notification.subject.r#type.as_str() {
        "PullRequest" => "Pull request",
        "Issue" => "Issue",
        "Discussion" => "Discussion",
        "CheckSuite" => "Check suite",
        other => other,
    };
    let meta_icon_class = match github_notification.subject.r#type.as_str() {
        "PullRequest" => "icon-[lucide--git-pull-request] size-4",
        "Discussion" => "icon-[lucide--message-square] size-4",
        "CheckSuite" => "icon-[lucide--check-circle] size-4",
        _ => "icon-[lucide--circle-dot] size-4",
    };
    let repo_name = github_notification.repository.full_name.clone();
    let repo_url = github_notification.repository.html_url.clone();
    let identifier = github_notification_id.map(|id| format!("#{id}"));
    let title = notification().title.clone();

    rsx! {
        div {
            class: "flex flex-col w-full h-full",

            PreviewCardHeader {
                brand_icon: rsx! { span { class: "{meta_icon_class}" } },
                title,
                identifier: identifier.clone(),
                subline: rsx! {
                    span { "{kind_label}" }
                    span { class: "sep", "·" }
                    a {
                        href: "{repo_url}",
                        target: "_blank",
                        rel: "noopener noreferrer",
                        "{repo_name}"
                    }
                }
            }

            div {
                class: "flex flex-col gap-2 w-full h-full overflow-y-auto scroll-y-auto p-3",

                Card {
                    variant: CardVariant::Default,
                    MetadataGrid {
                        MetadataItem {
                            label: "Repository".to_string(),
                            value: rsx! {
                                a {
                                    href: "{github_notification.repository.html_url.clone()}",
                                    target: "_blank",
                                    "{github_notification.repository.full_name}"
                                }
                                if let Some(id) = identifier.clone() {
                                    a {
                                        href: "{link}",
                                        target: "_blank",
                                        rel: "noopener noreferrer",
                                        "{id}"
                                    }
                                }
                            },
                        }

                        MetadataItem {
                            label: "Updated".to_string(),
                            value: rsx! { "{github_notification.updated_at}" },
                        }
                    }
                }
            }
        }
    }
}
