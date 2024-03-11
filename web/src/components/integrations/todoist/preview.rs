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

use crate::{
    components::{markdown::Markdown, SmallCard, TagsInCard},
    theme::{
        PRIORITY_HIGH_COLOR_CLASS, PRIORITY_LOW_COLOR_CLASS, PRIORITY_NORMAL_COLOR_CLASS,
        PRIORITY_URGENT_COLOR_CLASS,
    },
};

#[component]
pub fn TodoistTaskPreview<'a>(
    cx: Scope,
    notification: &'a NotificationWithTask,
    task: &'a Task,
    todoist_task: TodoistItem,
) -> Element {
    let link = notification.get_html_url();
    let project_link = task.get_html_project_url();
    let priority: u8 = task.priority.into();
    let task_priority_style = match todoist_task.priority {
        TodoistItemPriority::P1 => PRIORITY_LOW_COLOR_CLASS,
        TodoistItemPriority::P2 => PRIORITY_NORMAL_COLOR_CLASS,
        TodoistItemPriority::P3 => PRIORITY_HIGH_COLOR_CLASS,
        TodoistItemPriority::P4 => PRIORITY_URGENT_COLOR_CLASS,
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
                    Markdown { text: notification.title.clone() }
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

                div {
                    class: "flex text-gray-400 gap-1 text-xs",

                    "Created at ",
                    span { class: "text-primary", "{todoist_task.added_at}" }
                }

                TagsInCard {
                    tags: todoist_task
                        .labels
                        .iter()
                        .map(|label| label.clone().into())
                        .collect()
                }

                if let Some(due) = &todoist_task.due {
                    render! {
                        SmallCard {
                            Icon { class: "h-3 w-3", icon: BsCalendar2Check }
                            span { class: "text-gray-400", "Due date:" }
                            "{due.date}",
                            if due.is_recurring {
                                render! { Icon { class: "h-3 w-3", icon: BsArrowRepeat } }
                            }
                        }
                    }
                }

                SmallCard {
                    Icon { class: "h-3 w-3 {task_priority_style}", icon: BsFlag }
                    span { class: "text-gray-400", "Priority:" }
                    "{priority}"
                }
            }

            Markdown { text: task.body.clone() }
        }
    }
}
