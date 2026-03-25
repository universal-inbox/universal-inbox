#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::third_party::integrations::todoist::TodoistItem;

#[component]
pub fn TodoistListItemSubtitle(todoist_item: ReadSignal<TodoistItem>) -> Element {
    rsx! {
        if let Some(due) = todoist_item().due {
            span {
                class: "ui-nrow-meta-text flex items-center gap-1",

                span { class: "icon-[lucide--calendar-check] size-3" }
                span { "{due.date}" }
                if due.is_recurring {
                    span { class: "icon-[lucide--repeat-2] size-3" }
                }
            }
        }
    }
}
