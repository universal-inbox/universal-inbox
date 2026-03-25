#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::third_party::integrations::ticktick::TickTickItem;

#[component]
pub fn TickTickListItemSubtitle(ticktick_item: ReadSignal<TickTickItem>) -> Element {
    let due_date_str = ticktick_item()
        .due_date
        .map(|d| d.format("%Y-%m-%d").to_string());

    rsx! {
        if let Some(due_date) = due_date_str {
            span {
                class: "ui-nrow-meta-text flex items-center gap-1",

                span { class: "icon-[lucide--calendar-check] size-3" }
                span { "{due_date}" }
                if ticktick_item().is_recurring() {
                    span { class: "icon-[lucide--refresh-cw] size-3" }
                }
            }
        }
    }
}
