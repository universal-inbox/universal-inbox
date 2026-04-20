#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::bs_icons::{BsArrowRepeat, BsCalendar2Check},
};

use universal_inbox::third_party::integrations::ticktick::TickTickItem;

#[component]
pub fn TickTickListItemSubtitle(ticktick_item: ReadSignal<TickTickItem>) -> Element {
    let due_date_str = ticktick_item()
        .due_date
        .map(|d| d.format("%Y-%m-%d").to_string());

    rsx! {
        if let Some(due_date) = due_date_str {
            div {
                class: "flex items-center text-xs text-base-content/50 gap-1",

                Icon { class: "h-3 w-3", icon: BsCalendar2Check }
                span { "{due_date}" }
                if ticktick_item().is_recurring() {
                    Icon { class: "h-3 w-3", icon: BsArrowRepeat }
                }
            }
        }
    }
}
