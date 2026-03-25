#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{
    task::Task,
    third_party::integrations::ticktick::{TickTickItem, TickTickTaskStatus},
};

use crate::{
    components::{
        integrations::{
            icons::TickTick,
            ticktick::{list_item::TickTickListItemSubtitle, preview::ticktick_priority_level},
        },
        list::ListItem,
        priority_field::priority_color_class,
    },
    utils::format_elapsed_time,
};

#[component]
pub fn TickTickTaskListItem(
    task: ReadSignal<Task>,
    ticktick_item: ReadSignal<TickTickItem>,
    is_selected: ReadSignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let task_updated_at = use_memo(move || format_elapsed_time(task().updated_at));
    let priority = ticktick_item().priority;
    let meta_icon_color_class = ticktick_priority_level(priority)
        .map(priority_color_class)
        .unwrap_or("");
    let meta_icon_class = if ticktick_item().status == TickTickTaskStatus::Completed {
        "icon-[lucide--check-circle] w-full h-full"
    } else {
        "icon-[lucide--circle] w-full h-full"
    };

    rsx! {
        ListItem {
            key: "{task().id}",
            title: "{task().title}",
            subtitle: rsx! {
                TickTickListItemSubtitle { ticktick_item }
            },
            time: "{task_updated_at}",
            icon: rsx! { TickTick { class: "h-5 w-5" } },
            meta_icon: rsx! {
                span { class: "{meta_icon_class} {meta_icon_color_class}" }
            },
            is_selected,
            on_select,
        }
    }
}
