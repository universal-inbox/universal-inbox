#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{
        BsArrowRepeat, BsArrowUpRightSquare, BsCalendar2Check, BsCardChecklist, BsFlag,
    },
    Icon,
};

use universal_inbox::{
    notification::NotificationWithTask,
    task::integrations::todoist::{TodoistItem, TodoistItemPriority},
    task::Task,
    HasHtmlUrl,
};

use crate::components::CollapseCard;

#[inline_props]
pub fn TodoistTaskPreview<'a>(
    cx: Scope,
    notification: &'a NotificationWithTask,
    task: &'a Task,
    todoist_task: TodoistItem,
) -> Element {
    let link = notification.get_html_url();
    let project_link = task.get_html_project_url();
    let title = markdown::to_html(&notification.title);
    let body = markdown::to_html(&task.body);
    let priority: u8 = task.priority.into();
    let task_priority_style = match todoist_task.priority {
        TodoistItemPriority::P1 => "",
        TodoistItemPriority::P2 => "text-yellow-500",
        TodoistItemPriority::P3 => "text-orange-500",
        TodoistItemPriority::P4 => "text-red-500",
    };

    render! {
        div {
            class: "flex flex-col gap-2 w-full",

            div {
                class: "flex gap-2",

                a {
                    class: "text-xs text-gray-400",
                    href: "{project_link}",
                    target: "_blank",
                    "#{task.project}"
                }
            }

            h2 {
                class: "flex items-center gap-2 text-lg",

                Icon { class: "flex-none h-5 w-5 {task_priority_style}", icon: BsCardChecklist }
                a {
                    href: "{link}",
                    target: "_blank",
                    dangerous_inner_html: "{title}"
                }
                a {
                    class: "flex-none",
                    href: "{link}",
                    target: "_blank",
                    Icon { class: "h-5 w-5 text-gray-400 p-1", icon: BsArrowUpRightSquare }
                }
            }

            div {
                class: "flex flex-col gap-2 w-full",
                if let Some(due) = &todoist_task.due {
                    render! {
                        CollapseCard {
                            header: render! {
                                div {
                                    class: "flex items-center gap-2",

                                    Icon { class: "h-3 w-3", icon: BsCalendar2Check }
                                    span { class: "text-gray-400", "Due date:" }
                                    "{due.date}"
                                    if due.is_recurring {
                                        render! { Icon { class: "h-3 w-3", icon: BsArrowRepeat } }
                                    }
                                }
                            }
                        }
                    }
                }

                CollapseCard {
                    header: render! {
                        div {
                            class: "flex items-center gap-2",

                            Icon { class: "h-3 w-3 {task_priority_style}", icon: BsFlag }
                            span { class: "text-gray-400", "Priority:" }
                            "{priority}"
                        }
                    }
                }

                CollapseCard {
                    header: render! {
                        div {
                            class: "flex items-center gap-2",

                            span { class: "text-gray-400", "@" }
                            span { class: "text-gray-400", "Labels:" }
                            for label in &todoist_task.labels {
                                render! { span { "@{label}" } }
                            }
                        }
                    }
                }
            }

            p {
                class: "w-full prose prose-sm",
                dangerous_inner_html: "{body}"
            }
        }
    }
}
