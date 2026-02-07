#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{HasHtmlUrl, task::Task, third_party::integrations::linear::LinearIssue};

use crate::{
    components::{
        UserWithAvatar,
        integrations::linear::{
            icons::{Linear, LinearIssueIcon},
            list_item::LinearIssueListItemSubtitle,
        },
        list::{ListContext, ListItem},
        notifications_list::TaskHint,
        tasks_list::get_task_list_item_action_buttons,
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
    let list_context = use_context::<Memo<ListContext>>();
    let link = task().get_html_url();

    rsx! {
        ListItem {
            key: "{task().id}",
            title: "{linear_issue().title}",
            subtitle: rsx! { LinearIssueListItemSubtitle { linear_issue }},
            link,
            icon: rsx! {
                Linear { class: "h-5 w-5" }
                TaskHint { task: Some(task()) }
            },
            subicon: rsx! { LinearIssueIcon { class: "h-5 w-5 min-w-5", linear_issue } },
            action_buttons: get_task_list_item_action_buttons(
                task,
                list_context().show_shortcut,
                None,
                None,
            ),
            is_selected,
            on_select,

            if let Some(assignee) = linear_issue().assignee {
                UserWithAvatar { avatar_url: assignee.avatar_url.clone(), user_name: assignee.name.clone() }
            } else {
                UserWithAvatar {}
            }

            span { class: "text-base-content/50 whitespace-nowrap text-xs font-mono", "{task_updated_at}" }
        }
    }
}
