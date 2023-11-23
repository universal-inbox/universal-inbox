#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{BsArrowUpRightSquare, BsCalendar2Check, BsFlag},
    Icon,
};

use universal_inbox::notification::integrations::linear::{
    LinearIssue, LinearIssuePriority, LinearNotification, LinearProject,
};

use crate::{
    components::{
        integrations::linear::{
            icons::{LinearIssueIcon, LinearProjectIcon, LinearProjectMilestoneIcon},
            preview::project::LinearProjectDetails,
        },
        CollapseCard, SmallCard, TagsInCard, UserWithAvatar,
    },
    theme::{
        PRIORITY_HIGH_COLOR_CLASS, PRIORITY_LOW_COLOR_CLASS, PRIORITY_NORMAL_COLOR_CLASS,
        PRIORITY_URGENT_COLOR_CLASS,
    },
};

#[inline_props]
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

            LinearIssueDetails { linear_issue: linear_issue }
        }
    }
}

#[inline_props]
fn LinearIssueDetails<'a>(cx: Scope, linear_issue: &'a LinearIssue) -> Element {
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

            if let Some(creator) = &linear_issue.creator {
                render! {
                    SmallCard {
                        span { class: "text-gray-400", "Created by" }
                        UserWithAvatar {
                            user_name: creator.name.clone(),
                            avatar_url: creator.avatar_url.clone(),
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
                render! { LinearProjectCard { project: project } }
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

            if let Some(description) = &description {
                render! {
                    p {
                        class: "w-full prose prose-sm dark:prose-invert",
                        dangerous_inner_html: "{description}"
                    }
                }
            }
        }
    }
}

#[inline_props]
pub fn LinearProjectCard<'a>(cx: Scope, project: &'a LinearProject) -> Element {
    let project_icon = match &project.icon {
        Some(icon) => render! {
            img { class: "h-5 w-5", src: "{icon}" }
        },
        None => render! {
            div {
                style: "color: {project.color}",
                LinearProjectIcon { class: "h-5 w-5", linear_project: project }
            }
        },
    };

    render! {
        CollapseCard {
            header: render! {
                project_icon,
                span { class: "text-gray-400", "Project:" }
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
                card_class: "bg-neutral text-neutral-content",
                linear_project: project,
            }
        }
    }
}
