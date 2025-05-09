#![allow(non_snake_case)]

use chrono::{DateTime, Local};
use dioxus::prelude::*;

use dioxus_free_icons::{icons::bs_icons::BsCardChecklist, Icon};
use universal_inbox::{
    task::Task,
    third_party::integrations::todoist::{TodoistItem, TodoistItemPriority},
    HasHtmlUrl,
};

use crate::components::{
    integrations::todoist::{icons::Todoist, list_item::TodoistListItemSubtitle},
    list::{ListContext, ListItem},
    tasks_list::get_task_list_item_action_buttons,
    Tag, TagDisplay,
};

#[component]
pub fn TodoistTaskListItem(
    task: ReadOnlySignal<Task>,
    todoist_item: ReadOnlySignal<TodoistItem>,
    is_selected: ReadOnlySignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let task_updated_at = use_memo(move || {
        Into::<DateTime<Local>>::into(task().updated_at)
            .format("%Y-%m-%d %H:%M")
            .to_string()
    });
    let list_context = use_context::<Memo<ListContext>>();
    let task_icon_style = match todoist_item().priority {
        TodoistItemPriority::P1 => "",
        TodoistItemPriority::P2 => "text-yellow-500",
        TodoistItemPriority::P3 => "text-orange-500",
        TodoistItemPriority::P4 => "text-red-500",
    };
    let link = task().get_html_url();

    rsx! {
        ListItem {
            key: "{task().id}",
            title: "{task().title}",
            subtitle: rsx! { TodoistListItemSubtitle { todoist_item } },
            link,
            icon: rsx! { Todoist { class: "h-5 w-5" } },
            subicon: rsx! {
                Icon { class: "h-5 w-5 min-w-5 {task_icon_style}", icon: BsCardChecklist }
            },
            action_buttons: get_task_list_item_action_buttons(
                task,
                list_context().show_shortcut,
                None,
                None
            ),
            is_selected,
            on_select,

            for tag in todoist_item()
                .labels
                .iter()
                .map(|label| Into::<Tag>::into(label.clone())) {
                    TagDisplay { tag }
                }

            span { class: "text-base-content/50 whitespace-nowrap text-xs font-mono", "{task_updated_at}" }
        }
    }
}
