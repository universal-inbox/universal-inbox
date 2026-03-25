#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{task::Task, third_party::integrations::todoist::TodoistItem};

use crate::{
    components::{
        integrations::todoist::{
            icons::Todoist, list_item::TodoistListItemSubtitle, preview::todoist_priority_level,
        },
        list::ListItem,
        priority_field::priority_color_class,
    },
    utils::format_elapsed_time,
};

#[component]
pub fn TodoistTaskListItem(
    task: ReadSignal<Task>,
    todoist_item: ReadSignal<TodoistItem>,
    is_selected: ReadSignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let task_updated_at = use_memo(move || format_elapsed_time(task().updated_at));
    let priority = todoist_item().priority;
    let meta_icon_color_class = priority_color_class(todoist_priority_level(priority));
    let meta_icon_class = if todoist_item().checked {
        "icon-[lucide--check-circle] w-full h-full"
    } else {
        "icon-[lucide--circle] w-full h-full"
    };

    rsx! {
        ListItem {
            key: "{task().id}",
            title: "{task().title}",
            subtitle: rsx! {
                TodoistListItemSubtitle { todoist_item }
            },
            time: "{task_updated_at}",
            icon: rsx! { Todoist { class: "h-5 w-5" } },
            meta_icon: rsx! {
                span { class: "{meta_icon_class} {meta_icon_color_class}" }
            },
            is_selected,
            on_select,
        }
    }
}
