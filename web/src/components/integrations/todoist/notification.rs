#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{BsArrowRepeat, BsCalendar2Check, BsCardChecklist},
    Icon,
};

use universal_inbox::{
    notification::NotificationWithTask,
    task::integrations::todoist::{TodoistItem, TodoistItemPriority},
};

use crate::components::{Tag, TagDisplay};

#[component]
pub fn TodoistNotificationDisplay<'a>(
    cx: Scope,
    notif: &'a NotificationWithTask,
    todoist_task: TodoistItem,
) -> Element {
    let title = markdown::to_html(&notif.title);
    let task_icon_style = match todoist_task.priority {
        TodoistItemPriority::P1 => "",
        TodoistItemPriority::P2 => "text-yellow-500",
        TodoistItemPriority::P3 => "text-orange-500",
        TodoistItemPriority::P4 => "text-red-500",
    };

    render! {
        div {
            class: "flex items-center gap-2",

            Icon { class: "h-5 w-5 {task_icon_style}", icon: BsCardChecklist }

            div {
                class: "flex flex-col grow",

                span { dangerous_inner_html: "{title}" }
                div {
                    class: "flex gap-2",

                    if let Some(due) = &todoist_task.due {
                        render! {
                            div {
                                class: "flex items-center text-xs text-gray-400 gap-1",

                                Icon { class: "h-3 w-3", icon: BsCalendar2Check }
                                "{due.date}"
                                if due.is_recurring {
                                    render! { Icon { class: "h-3 w-3", icon: BsArrowRepeat } }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn TodoistNotificationDetailsDisplay<'a>(cx: Scope, todoist_item: &'a TodoistItem) -> Element {
    render! {
        div {
            class: "flex gap-2",

            for tag in todoist_item
                .labels
                .iter()
                .map(|label| Into::<Tag>::into(label.clone())) {
                    render! { TagDisplay { tag: tag } }
                }
        }
    }
}
