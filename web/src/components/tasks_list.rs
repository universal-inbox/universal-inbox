#![allow(non_snake_case)]

use sorted_groups::SortedGroups;

use dioxus::prelude::*;
use dioxus_free_icons::{Icon, icons::md_action_icons::MdCheckCircleOutline};

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
    pages::synced_tasks_page::TaskWithOrder,
    services::{task_service::TaskCommand, user_service::CONNECTED_USER},
};

#[derive(Clone, PartialEq)]
pub struct TaskListContext {
    pub is_task_actions_enabled: bool,
    pub is_read_only: bool,
    pub task_service: Coroutine<TaskCommand>,
}

#[component]
pub fn TasksList(tasks: ReadSignal<SortedGroups<String, TaskWithOrder>>) -> Element {
    let task_service = use_coroutine_handle::<TaskCommand>();
    let is_read_only = CONNECTED_USER
        .read()
        .as_ref()
        .map(|ctx| ctx.subscription.is_read_only)
        .unwrap_or(false);
    let context = use_memo(move || TaskListContext {
        is_task_actions_enabled: UI_MODEL.read().is_task_actions_enabled,
        is_read_only,
        task_service,
    });
    use_context_provider(move || context);
    let mut current_group = None;

    rsx! {
        div {
            class: "h-full overflow-y-auto scroll-y-auto px-2 snap-y snap-mandatory",
            List {
                id: "tasks-list",
                show_shortcut: UI_MODEL.read().is_help_enabled,

                for (i, (group, task)) in tasks.read().iter().enumerate() {
                    if current_group != Some(group) {
                        thead {
                            tr {
                                th {
                                    class: "flex flex-col px-0 pb-1 text-base-content/50 text-sm border-b snap-start",
                                    span { "{group}" }
                                }
                            }
                        }
                        { current_group = Some(group); }
                    }

                    tbody {
                        TaskListItem {
                            task: Signal::new(task.task.clone()),
                            is_selected: Some(i) == UI_MODEL.read().selected_task_index,
                            on_select: move |_| { UI_MODEL.write().selected_task_index = Some(i); },
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TaskListItem(
    task: ReadSignal<Task>,
    is_selected: ReadSignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    match task().source_item.data {
        ThirdPartyItemData::TodoistItem(todoist_item) => rsx! {
            TodoistTaskListItem {
                task,
                todoist_item: *todoist_item,
                is_selected,
                on_select,
            }
        },
        ThirdPartyItemData::LinearIssue(linear_issue) => rsx! {
            LinearTaskListItem {
                task,
                linear_issue: *linear_issue,
                is_selected,
                on_select,
            }
        },
        ThirdPartyItemData::SlackStar(slack_star) => rsx! {
            SlackStarTaskListItem {
                task,
                slack_star: *slack_star,
                is_selected,
                on_select,
            }
        },
        ThirdPartyItemData::SlackReaction(slack_reaction) => rsx! {
            SlackReactionTaskListItem {
                task,
                slack_reaction: *slack_reaction,
                is_selected,
                on_select,
            }
        },
        _ => rsx! {},
    }
}

fn get_task_action_disabled_label(
    is_read_only: bool,
    is_task_actions_enabled: bool,
) -> Option<Option<String>> {
    if is_read_only {
        Some(Some("Subscribe to perform this action".to_string()))
    } else if !is_task_actions_enabled {
        Some(Some("No task management service connected".to_string()))
    } else {
        None
    }
}

pub fn get_task_list_item_action_buttons(
    task: ReadSignal<Task>,
    show_shortcut: bool,
    button_class: Option<String>,
    container_class: Option<String>,
) -> Vec<Element> {
    let context = use_context::<Memo<TaskListContext>>();
    let is_read_only = context().is_read_only;
    let is_task_actions_enabled = context().is_task_actions_enabled;

    vec![rsx! {
        ListItemActionButton {
            title: "Complete task",
            shortcut: "c",
            disabled_label: get_task_action_disabled_label(is_read_only, is_task_actions_enabled),
            show_shortcut,
            button_class,
            container_class,
            onclick: move |_| {
                context().task_service
                    .send(TaskCommand::Complete(task().id));
            },
            Icon { class: "w-5 h-5", icon: MdCheckCircleOutline }
        }
    }]
}
