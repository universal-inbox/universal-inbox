#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::third_party::integrations::linear::{
    LinearComment, LinearIssue, LinearIssuePriority, LinearNotification,
};

use crate::{
    components::{
        Tag, TagDisplay, UserWithAvatar,
        integrations::linear::icons::{LinearIssueIcon, LinearProjectMilestoneIcon},
        markdown::Markdown,
        preview_card_header::PreviewCardHeader,
        priority_field::{PriorityField, PriorityLevel},
        thread::{Thread, ThreadChildren, ThreadItem},
        threaded_message::ThreadedMessage,
        ui::{Card, CardVariant, MetadataGrid, MetadataItem},
    },
    utils::format_elapsed_time,
};

fn linear_priority(issue: &LinearIssue) -> Option<(String, PriorityLevel)> {
    let level = match issue.priority {
        LinearIssuePriority::Urgent => PriorityLevel::Urgent,
        LinearIssuePriority::High => PriorityLevel::High,
        LinearIssuePriority::Normal => PriorityLevel::Normal,
        LinearIssuePriority::Low => PriorityLevel::Low,
        LinearIssuePriority::NoPriority => return None,
    };
    Some((issue.priority.to_string(), level))
}

#[component]
pub fn LinearIssuePreview(
    linear_issue: ReadSignal<LinearIssue>,
    linear_notification: ReadSignal<Option<LinearNotification>>,
    expand_details: ReadSignal<bool>,
) -> Element {
    let _ = expand_details;
    let issue = linear_issue();
    let identifier = format!("#{}", issue.identifier);
    let created_ago = format_elapsed_time(issue.created_at);
    let creator = issue.creator.clone();

    rsx! {
        div {
            class: "flex flex-col w-full h-full",

            PreviewCardHeader {
                brand_icon: rsx! { LinearIssueIcon { linear_issue, class: "size-4" } },
                title: linear_issue().title.clone(),
                identifier: Some(identifier),
                subline: rsx! {
                    if let Some(creator) = creator {
                        span { "Opened by" }
                        UserWithAvatar {
                            user_name: creator.name.clone(),
                            avatar_url: creator.avatar_url.clone(),
                            display_name: true,
                            class: "text-[11px]",
                        }
                        span { class: "sep", "·" }
                        span { "{created_ago} ago" }
                    } else {
                        span { "Opened {created_ago} ago" }
                    }
                }
            }

            LinearIssueDetails { linear_issue, linear_notification }
        }
    }
}

#[component]
fn LinearIssueDetails(
    linear_issue: ReadSignal<LinearIssue>,
    linear_notification: ReadSignal<Option<LinearNotification>>,
) -> Element {
    // status_color is `LinearWorkflowState.color` — already a hex string; strip any leading
    // '#' so we can inject it deterministically.
    let status_color = linear_issue()
        .state
        .color
        .trim_start_matches('#')
        .to_string();
    let status_pill_style = format!("background-color: #{status_color}; color: white;");

    rsx! {
        div {
            id: "notification-preview-details",
            class: "flex flex-col gap-2 w-full h-full overflow-y-auto scroll-y-auto p-3",

            Card {
                variant: CardVariant::Default,

                MetadataGrid {
                    MetadataItem {
                        label: "Status".to_string(),
                        value: rsx! {
                            LinearIssueIcon { linear_issue, class: "size-4" }
                            span {
                                class: "tag",
                                style: "{status_pill_style}",
                                "{linear_issue().state.name}"
                            }
                        },
                    }

                    if let Some((label, level)) = linear_priority(&linear_issue()) {
                        PriorityField { label, level }
                    }

                    if let Some(assignee) = linear_issue().assignee {
                        MetadataItem {
                            label: "Assigned to".to_string(),
                            value: rsx! {
                                UserWithAvatar {
                                    user_name: assignee.name.clone(),
                                    avatar_url: assignee.avatar_url.clone(),
                                    display_name: true,
                                }
                            },
                        }
                    }

                    if let Some(due_date) = linear_issue().due_date {
                        MetadataItem {
                            label: "Due date".to_string(),
                            value: rsx! {
                                span { class: "icon-[lucide--calendar-check] size-4" }
                                span { "{due_date}" }
                            },
                        }
                    }

                    if let Some(project_milestone) = linear_issue().project_milestone {
                        MetadataItem {
                            label: "Milestone".to_string(),
                            value: rsx! {
                                LinearProjectMilestoneIcon { class: "h-4 w-4" }
                                span { "{project_milestone.name}" }
                            },
                        }
                    }

                    if let Some(linear_project) = linear_issue().project {
                        MetadataItem {
                            label: "Project".to_string(),
                            value: rsx! {
                                if let Some(icon) = linear_project.icon.clone() {
                                    span { "{icon}" }
                                }
                                a {
                                    href: "{linear_project.url}",
                                    target: "_blank",
                                    "{linear_project.name}"
                                }
                            },
                        }
                    }

                    if !linear_issue().labels.is_empty() {
                        MetadataItem {
                            label: "Labels".to_string(),
                            value: rsx! {
                                for label in linear_issue().labels {
                                    TagDisplay { tag: Into::<Tag>::into(label) }
                                }
                            },
                        }
                    }
                }
            }

            if let Some(description) = linear_issue().description {
                Card {
                    variant: CardVariant::Default,
                    Markdown {
                        class: "prose prose-sm w-full max-w-full",
                        text: description.clone()
                    }
                }
            }

            if let Some(LinearNotification::IssueNotification { comment: Some(linear_comment), .. }) = linear_notification() {
                Card {
                    variant: CardVariant::Default,

                    Thread {
                        LinearCommentThread { linear_comment }
                    }
                }
            }
        }
    }
}

/// One row of the Linear comment thread. Renders the author header, the body,
/// and (when present) a [`ThreadChildren`] block recursing into each child.
#[component]
fn LinearCommentThread(linear_comment: ReadSignal<LinearComment>) -> Element {
    let comment = linear_comment();
    let (author_name, author_avatar_url) = match comment.user.clone() {
        Some(user) => (user.name, user.avatar_url),
        None => ("Unknown".to_string(), None),
    };
    let body_text = comment.body.clone();

    rsx! {
        ThreadItem {
            ThreadedMessage {
                author_name,
                author_avatar_url,
                sent_at: Some(comment.updated_at),
                body: rsx! {
                    Markdown {
                        class: "prose prose-sm w-full max-w-full",
                        text: body_text
                    }
                },
            }

            if !comment.children.is_empty() {
                ThreadChildren {
                    for child_comment in comment.children.clone().into_iter() {
                        LinearCommentThread { linear_comment: child_comment }
                    }
                }
            }
        }
    }
}
