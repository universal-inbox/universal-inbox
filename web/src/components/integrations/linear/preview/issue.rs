#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::bs_icons::{BsArrowUpRightSquare, BsCalendar2Check, BsFlag},
};

use universal_inbox::third_party::integrations::linear::{
    LinearComment, LinearIssue, LinearIssuePriority, LinearNotification, LinearProject,
};

use crate::{
    components::{
        CollapseCard, MessageHeader, SmallCard, Tag, TagDisplay, TagsInCard, UserWithAvatar,
        integrations::linear::{
            get_notification_type_label,
            icons::{LinearIssueIcon, LinearProjectIcon, LinearProjectMilestoneIcon},
            preview::project::LinearProjectDetails,
        },
        markdown::Markdown,
    },
    theme::{
        PRIORITY_HIGH_COLOR_CLASS, PRIORITY_LOW_COLOR_CLASS, PRIORITY_NORMAL_COLOR_CLASS,
        PRIORITY_URGENT_COLOR_CLASS,
    },
};

#[component]
pub fn LinearIssuePreview(
    linear_issue: ReadSignal<LinearIssue>,
    linear_notification: ReadSignal<Option<LinearNotification>>,
    expand_details: ReadSignal<bool>,
) -> Element {
    rsx! {
        div {
            class: "flex flex-col gap-2 w-full h-full",

            h3 {
                class: "flex items-center gap-2 text-base",

                LinearIssueIcon { class: "h-5 w-5", linear_issue: linear_issue }
                a {
                    class: "flex items-center",
                    href: "{linear_issue().url}",
                    target: "_blank",
                    Markdown { text: linear_issue().title.clone() }
                    Icon { class: "h-5 w-5 min-w-5 text-base-content/50 p-1", icon: BsArrowUpRightSquare }
                }
            }

            LinearIssueDetails { linear_issue, linear_notification, expand_details }
        }
    }
}

#[component]
fn LinearIssueDetails(
    linear_issue: ReadSignal<LinearIssue>,
    linear_notification: ReadSignal<Option<LinearNotification>>,
    expand_details: ReadSignal<bool>,
) -> Element {
    let issue_priority_style = match linear_issue().priority {
        LinearIssuePriority::Low => PRIORITY_LOW_COLOR_CLASS,
        LinearIssuePriority::Normal => PRIORITY_NORMAL_COLOR_CLASS,
        LinearIssuePriority::High => PRIORITY_HIGH_COLOR_CLASS,
        LinearIssuePriority::Urgent => PRIORITY_URGENT_COLOR_CLASS,
        _ => "",
    };

    rsx! {
        div {
            id: "notification-preview-details",
            class: "flex flex-col gap-2 w-full h-full overflow-y-auto scroll-y-auto",

            div {
                class: "flex gap-2",

                if let Some(linear_notification) = linear_notification() {
                    a {
                        class: "text-xs text-base-content/50",
                        href: "{linear_issue().team.get_url(linear_notification.get_organization())}",
                        target: "_blank",
                        "{linear_issue().team.name}"
                    }
                } else {
                    span { class: "text-xs text-base-content/50", "{linear_issue().team.name}" }
                }

                a {
                    class: "text-xs text-base-content/50",
                    href: "{linear_issue().url}",
                    target: "_blank",
                    "#{linear_issue().identifier} "
                }
            }

            div {
                class: "flex text-base-content/50 gap-1 text-xs",

                "Created at ",
                span { class: "text-primary", "{linear_issue().created_at}" }
            }

            if let Some(description) = linear_issue().description {
                CollapseCard {
                    id: "linear-issue-details",
                    header: rsx! { span { class: "text-base-content/50", "Description" } },
                    opened: expand_details(),
                    Markdown {
                        class: "prose prose-sm w-full max-w-full",
                        text: description.clone()
                    }
                }
            }

            if let Some(linear_notification) = linear_notification() {
                SmallCard {
                    span { class: "text-base-content/50", "Reason:" }
                    TagDisplay {
                        tag: Into::<Tag>::into(get_notification_type_label(&linear_notification.get_type()))
                    }
                }
            }

            if let Some(creator) = linear_issue().creator {
                SmallCard {
                    span { class: "text-base-content/50", "Created by" }
                    UserWithAvatar {
                        user_name: creator.name.clone(),
                        avatar_url: creator.avatar_url.clone(),
                        display_name: true
                    }
                }
            }

            if let Some(assignee) = linear_issue().assignee {
                SmallCard {
                    span { class: "text-base-content/50", "Assigned to" }
                    UserWithAvatar {
                        user_name: assignee.name.clone(),
                        avatar_url: assignee.avatar_url.clone(),
                        display_name: true
                    }
                }
            }

            SmallCard {
                LinearIssueIcon { class: "h-5 w-5", linear_issue: linear_issue }
                span { "{linear_issue().state.name}" }
            }

            TagsInCard {
                tags: linear_issue()
                    .labels
                    .iter()
                    .map(|label| label.clone().into())
                    .collect()
            }

            if let Some(due_date) = linear_issue().due_date {
                SmallCard {
                    Icon { class: "h-5 w-5", icon: BsCalendar2Check }
                    span { class: "text-base-content/50", "Due date:" }
                    "{due_date}"
                }
            }

            if linear_issue().priority != LinearIssuePriority::NoPriority {
                SmallCard {
                    Icon { class: "h-5 w-5 {issue_priority_style}", icon: BsFlag }
                    span { class: "text-base-content/50", "Priority:" }
                    "{linear_issue().priority}"
                }
            }

            if let Some(linear_project) = linear_issue().project {
                LinearProjectCard { linear_project, linear_notification, expand_details }
            }

            if let Some(project_milestone) = linear_issue().project_milestone {
                SmallCard {
                    LinearProjectMilestoneIcon { class: "h-5 w-5" }
                    span { class: "text-base-content/50", "Milestone:" }
                    "{project_milestone.name}"
                }
            }

            if let Some(LinearNotification::IssueNotification { comment: Some(linear_comment), .. }) = linear_notification() {
                div {
                    class: "card w-full bg-base-200",
                    div {
                        class: "card-body flex flex-col gap-2 p-2",
                        LinearCommentDisplay { linear_comment }
                    }
                }
            }
        }
    }
}

#[component]
pub fn LinearProjectCard(
    linear_project: ReadSignal<LinearProject>,
    linear_notification: ReadSignal<Option<LinearNotification>>,
    expand_details: ReadSignal<bool>,
) -> Element {
    rsx! {
        CollapseCard {
            id: "linear-project",
            header: rsx! {
                div {
                    style: "color: {linear_project().color}",
                    LinearProjectIcon { class: "h-5 w-5", linear_project }
                },
                span { class: "text-base-content/50", "Project:" }

                if let Some(icon) = linear_project().icon {
                    span { "{icon}" }
                }
                a {
                    href: "{linear_project().url}",
                    target: "_blank",
                    "{linear_project().name}"
                }
                a {
                    class: "flex-none",
                    href: "{linear_project().url}",
                    target: "_blank",
                    Icon { class: "h-5 w-5 min-w-5 text-base-content/50 p-1", icon: BsArrowUpRightSquare }
                }
            },
            opened: expand_details(),

            LinearProjectDetails {
                linear_project,
                linear_notification,
                expand_details,
                dark_bg: true
            }
        }
    }
}

#[component]
fn LinearCommentDisplay(
    linear_comment: ReadSignal<LinearComment>,
    class: Option<String>,
) -> Element {
    let class = class.unwrap_or_default();

    rsx! {
        div {
            class: "flex flex-col gap-2 {class}",

            if let Some(user) = linear_comment().user {
                SmallCard {
                    class: "flex flex-row items-center gap-2",
                    card_class: "bg-neutral text-neutral-content text-xs",

                    MessageHeader {
                        user_name: user.name.clone(),
                        avatar_url: user.avatar_url.clone(),
                        display_name: true,
                        sent_at: Some(linear_comment().updated_at)
                    }
                    // span { class: "text-neutral-content/75", "From" }
                    // UserWithAvatar {
                    //     class: "text-xs",
                    //     user_name: user.name.clone(),
                    //     avatar_url: user.avatar_url.clone(),
                    //     display_name: true
                    // }
                }
                // span { class: "text-neutral-content/75", "at" }
                // span { " {updated_at}" }
            }

            Markdown {
                class: "prose prose-sm w-full max-w-full",
                text: linear_comment().body.clone()
            }

            for child_comment in linear_comment().children.into_iter() {
                LinearCommentDisplay {
                    class: "pl-2",
                    linear_comment: child_comment
                }
            }
        }
    }
}
