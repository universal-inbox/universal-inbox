#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::bs_icons::{
        BsArrowRepeat, BsArrowUpRightSquare, BsCalendar2Check, BsCardChecklist, BsFlag,
    },
};

use universal_inbox::{
    HasHtmlUrl,
    task::Task,
    third_party::integrations::ticktick::{TickTickItem, TickTickItemPriority},
};

use crate::{
    components::{SmallCard, TagsInCard, markdown::Markdown},
    theme::{
        PRIORITY_HIGH_COLOR_CLASS, PRIORITY_LOW_COLOR_CLASS, PRIORITY_NORMAL_COLOR_CLASS,
        PRIORITY_URGENT_COLOR_CLASS,
    },
};

#[component]
pub fn TickTickTaskPreview(
    task: ReadSignal<Task>,
    ticktick_item: ReadSignal<TickTickItem>,
) -> Element {
    let link = task().get_html_url();
    let project_link = task().get_html_project_url();
    let priority: u8 = task().priority.into();
    let task_priority_style = match ticktick_item().priority {
        TickTickItemPriority::High => PRIORITY_URGENT_COLOR_CLASS,
        TickTickItemPriority::Medium => PRIORITY_HIGH_COLOR_CLASS,
        TickTickItemPriority::Low => PRIORITY_NORMAL_COLOR_CLASS,
        TickTickItemPriority::None => PRIORITY_LOW_COLOR_CLASS,
    };

    let created_time_str = ticktick_item()
        .created_time
        .map(|t| t.format("%Y-%m-%d %H:%M").to_string());
    let due_date_str = ticktick_item()
        .due_date
        .map(|d| d.format("%Y-%m-%d").to_string());

    rsx! {
        div {
            class: "flex flex-col gap-2 w-full h-full",

            h3 {
                class: "flex items-center gap-2 text-base",

                Icon { class: "flex-none h-5 w-5 {task_priority_style}", icon: BsCardChecklist }
                a {
                    class: "flex items-center",
                    href: "{link}",
                    target: "_blank",
                    Markdown { text: task().title.clone() }
                    Icon { class: "h-5 w-5 min-w-5 text-base-content/50 p-1", icon: BsArrowUpRightSquare }
                }
            }

            div {
                id: "task-preview-details",
                class: "flex flex-col gap-2 w-full h-full overflow-y-auto scroll-y-auto",

                div {
                    class: "flex gap-2",

                    a {
                        class: "text-xs text-base-content/50",
                        href: "{project_link}",
                        target: "_blank",
                        "#{task().project}"
                    }
                }

                if let Some(created_time) = &created_time_str {
                    div {
                        class: "flex text-base-content/50 gap-1 text-xs",

                        "Created at ",
                        span { class: "text-primary", "{created_time}" }
                    }
                }

                TagsInCard {
                    tags: ticktick_item()
                        .tags
                        .unwrap_or_default()
                        .into_iter()
                        .map(|tag_name| tag_name.into())
                        .collect()
                }

                if let Some(due_date) = &due_date_str {
                    SmallCard {
                        Icon { class: "h-3 w-3", icon: BsCalendar2Check }
                        span { class: "text-base-content/50", "Due date:" }
                        "{due_date}",
                        if ticktick_item().is_recurring() {
                            Icon { class: "h-3 w-3", icon: BsArrowRepeat }
                        }
                    }
                }

                SmallCard {
                    Icon { class: "h-3 w-3 {task_priority_style}", icon: BsFlag }
                    span { class: "text-base-content/50", "Priority:" }
                    "{priority}"
                }

                Markdown {
                    class: "prose prose-sm w-full max-w-full",
                    text: task().body
                }
            }
        }
    }
}
