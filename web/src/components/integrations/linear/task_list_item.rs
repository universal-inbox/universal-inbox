#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{task::Task, third_party::integrations::linear::LinearIssue};

use crate::{
    components::{
        integrations::linear::{icons::Linear, list_item::LinearIssueListItemSubtitle},
        list::ListItem,
    },
    utils::format_elapsed_time,
};

#[component]
pub fn LinearTaskListItem(
    task: ReadSignal<Task>,
    linear_issue: ReadSignal<LinearIssue>,
    is_selected: ReadSignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let task_updated_at = use_memo(move || format_elapsed_time(task().updated_at));

    rsx! {
        ListItem {
            key: "{task().id}",
            // The task manager owns the title, so show the task's own title
            // rather than re-reading the raw issue payload.
            title: "{task().title}",
            subtitle: rsx! {
                LinearIssueListItemSubtitle { linear_issue }
            },
            time: "{task_updated_at}",
            icon: rsx! {
                Linear { class: "h-5 w-5" }
            },
            meta_icon: rsx! { span { class: "icon-[lucide--circle-dot] w-full h-full" } },
            is_selected,
            on_select,
        }
    }
}
