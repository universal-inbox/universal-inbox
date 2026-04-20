#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{Icon, icons::bs_icons::BsCardChecklist};

use universal_inbox::{
    HasHtmlUrl,
    task::Task,
    third_party::integrations::ticktick::{TickTickItem, TickTickItemPriority},
};

use crate::{
    components::{
        Tag, TagDisplay,
        integrations::{icons::TickTick, ticktick::list_item::TickTickListItemSubtitle},
        list::{ListContext, ListItem},
        tasks_list::get_task_list_item_action_buttons,
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
    let list_context = use_context::<Memo<ListContext>>();
    let task_icon_style = match ticktick_item().priority {
        TickTickItemPriority::High => "",
        TickTickItemPriority::Medium => "text-yellow-500",
        TickTickItemPriority::Low => "text-orange-500",
        TickTickItemPriority::None => "",
    };
    let link = task().get_html_url();

    rsx! {
        ListItem {
            key: "{task().id}",
            title: "{task().title}",
            subtitle: rsx! { TickTickListItemSubtitle { ticktick_item } },
            link,
            icon: rsx! { TickTick { class: "h-5 w-5" } },
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

            for tag in ticktick_item()
                .tags
                .unwrap_or_default()
                .into_iter()
                .map(Into::<Tag>::into) {
                    TagDisplay { tag }
                }

            span { class: "text-base-content/50 whitespace-nowrap text-xs font-mono", "{task_updated_at}" }
        }
    }
}
