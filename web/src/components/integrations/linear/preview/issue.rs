#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{BsArrowUpRightSquare, BsCalendar2Check, BsFlag},
    Icon,
};

use universal_inbox::notification::integrations::linear::{
    LinearComment, LinearIssue, LinearIssuePriority, LinearNotification, LinearProject,
};

use crate::{
    components::{
        integrations::linear::{
            get_notification_type_label,
            icons::{LinearIssueIcon, LinearProjectIcon, LinearProjectMilestoneIcon},
            preview::project::LinearProjectDetails,
        },
        CollapseCard, SmallCard, Tag, TagDisplay, TagsInCard, UserWithAvatar,
    },
    theme::{
        PRIORITY_HIGH_COLOR_CLASS, PRIORITY_LOW_COLOR_CLASS, PRIORITY_NORMAL_COLOR_CLASS,
        PRIORITY_URGENT_COLOR_CLASS,
    },
};

#[component]
pub fn LinearIssuePreview<'a>(
    cx: Scope,
    linear_notification: &'a LinearNotification,
    linear_issue: &'a LinearIssue,
) -> Element {
    let title = markdown::to_html(&linear_issue.title);

    render! {
        div {
            class: "flex flex-col gap-2 w-full",

            div {
                class: "flex gap-2",

                a {
                    class: "text-xs text-gray-400",
                    href: "{linear_issue.team.get_url(linear_notification.get_organization())}",
                    target: "_blank",
                    "{linear_issue.team.name}"
                }

                a {
                    class: "text-xs text-gray-400",
                    href: "{linear_issue.url}",
                    target: "_blank",
                    "#{linear_issue.identifier} "
                }
            }

            h2 {
                class: "flex items-center gap-2 text-lg",

                LinearIssueIcon { class: "h-5 w-5", linear_issue: linear_issue }
                a {
                    href: "{linear_issue.url}",
                    target: "_blank",
                    dangerous_inner_html: "{title}"
                }
                a {
                    class: "flex-none",
                    href: "{linear_issue.url}",
                    target: "_blank",
                    Icon { class: "h-5 w-5 text-gray-400 p-1", icon: BsArrowUpRightSquare }
                }
            }

            LinearIssueDetails {
                linear_notification: linear_notification,
                linear_issue: linear_issue
            }
        }
    }
}

#[component]
fn LinearIssueDetails<'a>(
    cx: Scope,
    linear_notification: &'a LinearNotification,
    linear_issue: &'a LinearIssue,
) -> Element {
    let description = linear_issue
        .description
        .as_ref()
        .map(|description| markdown::to_html(description));
    let issue_priority_style = match linear_issue.priority {
        LinearIssuePriority::Low => PRIORITY_LOW_COLOR_CLASS,
        LinearIssuePriority::Normal => PRIORITY_NORMAL_COLOR_CLASS,
        LinearIssuePriority::High => PRIORITY_HIGH_COLOR_CLASS,
        LinearIssuePriority::Urgent => PRIORITY_URGENT_COLOR_CLASS,
        _ => "",
    };

    render! {
        div {
            class: "flex flex-col gap-2 w-full",

            div {
                class: "flex text-gray-400 gap-1 text-xs",

                "Created at ",
                span { class: "text-primary", "{linear_issue.created_at}" }
            }

            if let Some(description) = &description {
                render! {
                    CollapseCard {
                        header: render! { span { class: "text-gray-400", "Description" } },
                        p {
                            class: "w-full prose prose-sm dark:prose-invert",
                            dangerous_inner_html: "{description}"
                        }
                    }
                }
            }

            SmallCard {
                span { class: "text-gray-400", "Reason:" }
                TagDisplay {
                    tag: Into::<Tag>::into(get_notification_type_label(&linear_notification.get_type()))
                }
            }

            if let Some(creator) = &linear_issue.creator {
                render! {
                    SmallCard {
                        span { class: "text-gray-400", "Created by" }
                        UserWithAvatar {
                            user_name: creator.name.clone(),
                            avatar_url: creator.avatar_url.clone(),
                            initials_from: creator.name.clone(),
                        }
                    }
                }
            }

            if let Some(assignee) = &linear_issue.assignee {
                render! {
                    SmallCard {
                        span { class: "text-gray-400", "Assigned to" }
                        UserWithAvatar {
                            user_name: assignee.name.clone(),
                            avatar_url: assignee.avatar_url.clone(),
                            initials_from: assignee.name.clone(),
                        }
                    }
                }
            }

            SmallCard {
                LinearIssueIcon { class: "h-5 w-5", linear_issue: linear_issue }
                span { "{linear_issue.state.name}" }
            }

            TagsInCard {
                tags: linear_issue
                    .labels
                    .iter()
                    .map(|label| label.clone().into())
                    .collect()
            }

            if let Some(due_date) = &linear_issue.due_date {
                render! {
                    SmallCard {
                        Icon { class: "h-5 w-5", icon: BsCalendar2Check }
                        span { class: "text-gray-400", "Due date:" }
                        "{due_date}"
                    }
                }
            }

            if linear_issue.priority != LinearIssuePriority::NoPriority {
                render! {
                    SmallCard {
                        Icon { class: "h-5 w-5 {issue_priority_style}", icon: BsFlag }
                        span { class: "text-gray-400", "Priority:" }
                        "{linear_issue.priority}"
                    }
                }
            }

            if let Some(project) = &linear_issue.project {
                render! { LinearProjectCard { linear_notification: linear_notification, project: project } }
            }

            if let Some(project_milestone) = &linear_issue.project_milestone {
                render! {
                    SmallCard {
                        LinearProjectMilestoneIcon { class: "h-5 w-5" }
                        span { class: "text-gray-400", "Milestone:" }
                        "{project_milestone.name}"
                    }
                }
            }

            if let LinearNotification::IssueNotification { comment: Some(comment), .. } = linear_notification {
                render! {
                    div {
                        class: "card w-full bg-base-200 text-base-content",
                        div {
                            class: "card-body flex flex-col gap-2 p-2",
                            LinearCommentDisplay { linear_comment: comment }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn LinearProjectCard<'a>(
    cx: Scope,
    linear_notification: &'a LinearNotification,
    project: &'a LinearProject,
) -> Element {
    render! {
        CollapseCard {
            header: render! {
                div {
                    style: "color: {project.color}",
                    LinearProjectIcon { class: "h-5 w-5", linear_project: project }
                },
                span { class: "text-gray-400", "Project:" }

                if let Some(icon) = &project.icon {
                    render! { span { "{icon}" } }
                }
                a {
                    href: "{project.url}",
                    target: "_blank",
                    "{project.name}"
                }
                a {
                    class: "flex-none",
                    href: "{project.url}",
                    target: "_blank",
                    Icon { class: "h-5 w-5 text-gray-400 p-1", icon: BsArrowUpRightSquare }
                }
            },

            LinearProjectDetails {
                linear_notification: linear_notification,
                linear_project: project,
                dark_bg: true
            }
        }
    }
}

#[component]
fn LinearCommentDisplay<'a>(
    cx: Scope,
    linear_comment: &'a LinearComment,
    class: Option<&'a str>,
) -> Element {
    let comment_body = markdown::to_html(&linear_comment.body);
    let updated_at = linear_comment
        .updated_at
        .format("%Y-%m-%d %H:%M")
        .to_string();

    render! {
        div {
            class: "flex flex-col gap-2 {class.unwrap_or_default()}",

            SmallCard {
                class: "flex flex-row items-center gap-2 text-xs",
                card_class: "bg-neutral text-neutral-content",

                if let Some(user) = &linear_comment.user {
                    render! {
                        span { class: "text-gray-400", "From" }
                        UserWithAvatar {
                            user_name: user.name.clone(),
                            avatar_url: user.avatar_url.clone(),
                            initials_from: user.name.clone(),
                        }
                    }
                }
                span { class: "text-gray-400", "on" }
                span { " {updated_at}" }
            }

            p {
                class: "w-full prose prose-sm dark:prose-invert",
                dangerous_inner_html: "{comment_body}"
            }

            for child_comment in linear_comment.children.iter() {
                render! {
                    LinearCommentDisplay {
                        class: "pl-2",
                        linear_comment: child_comment
                    }
                }
            }
        }
    }
}
