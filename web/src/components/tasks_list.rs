#![allow(non_snake_case)]

use dioxus::prelude::*;

use dioxus_free_icons::{icons::bs_icons::BsCheck2, Icon};
use universal_inbox::{task::Task, third_party::item::ThirdPartyItemData};

use crate::{
    components::{
        integrations::{
            linear::task_list_item::LinearTaskListItem,
            slack::task_list_item::{SlackReactionTaskListItem, SlackStarTaskListItem},
            todoist::task_list_item::TodoistTaskListItem,
        },
        list::{List, ListItemActionButton},
    },
    model::UI_MODEL,
    services::task_service::TaskCommand,
};

#[derive(Clone, PartialEq)]
pub struct TaskListContext {
    pub is_task_actions_enabled: bool,
    pub task_service: Coroutine<TaskCommand>,
}

#[component]
pub fn TasksList(tasks: ReadOnlySignal<Vec<Task>>) -> Element {
    let task_service = use_coroutine_handle::<TaskCommand>();
    let context = use_memo(move || TaskListContext {
        is_task_actions_enabled: UI_MODEL.read().is_task_actions_enabled,
        task_service,
    });
    use_context_provider(move || context);

    rsx! {
        List {
            id: "tasks_list",
            show_shortcut: UI_MODEL.read().is_help_enabled,

            for (i, task) in tasks().into_iter().map(Signal::new).enumerate() {
                TaskListItem {
                    task,
                    is_selected: i == UI_MODEL.read().selected_task_index,
                    on_select: move |_| {
                        UI_MODEL.write().selected_task_index = i;
                    },
                }
            }
        }
    }
}

#[component]
fn TaskListItem(
    task: ReadOnlySignal<Task>,
    is_selected: ReadOnlySignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    match task().source_item.data {
        ThirdPartyItemData::TodoistItem(todoist_item) => rsx! {
            TodoistTaskListItem {
                task,
                todoist_item,
                is_selected,
                on_select,
            }
        },
        ThirdPartyItemData::LinearIssue(linear_issue) => rsx! {
            LinearTaskListItem {
                task,
                linear_issue,
                is_selected,
                on_select,
            }
        },
        ThirdPartyItemData::SlackStar(slack_star) => rsx! {
            SlackStarTaskListItem {
                task,
                slack_star,
                is_selected,
                on_select,
            }
        },
        ThirdPartyItemData::SlackReaction(slack_reaction) => rsx! {
            SlackReactionTaskListItem {
                task,
                slack_reaction,
                is_selected,
                on_select,
            }
        },
    }
}

pub fn get_task_list_item_action_buttons(
    task: ReadOnlySignal<Task>,
    show_shortcut: bool,
) -> Vec<Element> {
    let context = use_context::<Memo<TaskListContext>>();

    vec![rsx! {
        ListItemActionButton {
            title: "Complete task",
            shortcut: "c",
            disabled_label: (!context().is_task_actions_enabled)
                .then_some("No task management service connected".to_string()),
            show_shortcut,
            onclick: move |_| {
                context().task_service
                    .send(TaskCommand::Complete(task().id));
            },
            Icon { class: "w-5 h-5", icon: BsCheck2 }
        }
    }]
}
