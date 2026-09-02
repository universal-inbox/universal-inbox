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
            // Show the title Linear seeded the task with rather than the stored
            // one: a row sits next to Linear's own icon, so a name the user
            // gave the task in their task manager would read as the wrong
            // title there. The rename shows up in the preview header instead.
            title: "{linear_issue().render_task_title()}",
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
