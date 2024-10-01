#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{BsArrowRepeat, BsCalendar2Check},
    Icon,
};

use universal_inbox::third_party::integrations::todoist::TodoistItem;

#[component]
pub fn TodoistListItemSubtitle(todoist_item: ReadOnlySignal<TodoistItem>) -> Element {
    rsx! {
        if let Some(due) = todoist_item().due {
            div {
                class: "flex items-center text-xs text-gray-400 gap-1",

                Icon { class: "h-3 w-3", icon: BsCalendar2Check }
                span { "{due.date}" }
                if due.is_recurring {
                    Icon { class: "h-3 w-3", icon: BsArrowRepeat }
                }
            }
        }
    }
}
