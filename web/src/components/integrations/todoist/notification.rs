#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{BsArrowRepeat, BsCalendar2Check, BsCardChecklist},
    Icon,
};

use universal_inbox::{
    notification::NotificationWithTask,
    third_party::integrations::todoist::{TodoistItem, TodoistItemPriority},
};

use crate::components::{markdown::Markdown, Tag, TagDisplay};

#[component]
pub fn TodoistNotificationDisplay(
    notif: ReadOnlySignal<NotificationWithTask>,
    todoist_task: TodoistItem,
) -> Element {
    let task_icon_style = match todoist_task.priority {
        TodoistItemPriority::P1 => "",
        TodoistItemPriority::P2 => "text-yellow-500",
        TodoistItemPriority::P3 => "text-orange-500",
        TodoistItemPriority::P4 => "text-red-500",
    };

    rsx! {
        div {
            class: "flex items-center gap-2",

            Icon { class: "h-5 w-5 min-w-5 {task_icon_style}", icon: BsCardChecklist }

            div {
                class: "flex flex-col grow",

                Markdown { text: notif().title.clone() }
                div {
                    class: "flex gap-2",

                    if let Some(due) = &todoist_task.due {
                        div {
                            class: "flex items-center text-xs text-gray-400 gap-1",

                            Icon { class: "h-3 w-3", icon: BsCalendar2Check }
                            "{due.date}"
                            if due.is_recurring {
                                Icon { class: "h-3 w-3", icon: BsArrowRepeat }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn TodoistNotificationDetailsDisplay(todoist_item: ReadOnlySignal<TodoistItem>) -> Element {
    rsx! {
        div {
            class: "flex gap-2",

            for tag in todoist_item()
                .labels
                .iter()
                .map(|label| Into::<Tag>::into(label.clone())) {
                    TagDisplay { tag: tag }
                }
        }
    }
}
