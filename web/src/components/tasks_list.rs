#![allow(non_snake_case)]

use sorted_groups::SortedGroups;

use dioxus::prelude::*;

use universal_inbox::{task::Task, third_party::item::ThirdPartyItemData};

use crate::{
    components::{
        integrations::{
            linear::task_list_item::LinearTaskListItem,
            slack::task_list_item::SlackReactionTaskListItem,
            ticktick::task_list_item::TickTickTaskListItem,
            todoist::task_list_item::TodoistTaskListItem,
        },
        list::List,
    },
    model::UI_MODEL,
    pages::synced_tasks_page::TaskWithOrder,
    services::task_service::TaskCommand,
};

#[derive(Clone, PartialEq)]
pub struct TaskListContext {
    pub is_task_actions_enabled: bool,
    pub task_service: Coroutine<TaskCommand>,
}

#[component]
pub fn TasksList(tasks: ReadSignal<SortedGroups<String, TaskWithOrder>>) -> Element {
    let task_service = use_coroutine_handle::<TaskCommand>();
    let context = use_memo(move || TaskListContext {
        is_task_actions_enabled: UI_MODEL.read().is_task_actions_enabled,
        task_service,
    });
    use_context_provider(move || context);
    let mut current_group = None;

    rsx! {
        div {
            id: "tasks-list",
            // Mirrors `notifications_list.rs`: shell properties stay in CSS,
            // responsive width sits on the element via `max-*` variants with
            // `!` (`!important`) to win against the `.list-panel` rule.
            class: "list-panel max-xl:w-[360px]! max-xl:min-w-[300px]! max-lg:w-[320px]! max-lg:min-w-[280px]! max-md:w-full! max-md:min-w-0! max-md:[.app-layout.show-detail_&]:hidden!",

            div {
                class: "flex items-center justify-between py-1.5 px-5 border-b border-ui-border bg-ui-surface",
                h1 {
                    class: "flex items-center gap-2 text-[15px] font-bold tracking-tight",
                    "Tasks"
                    if !tasks.read().is_empty() {
                        span {
                            class: "text-xs font-medium text-ui-base-muted px-0.5",
                            "{tasks.read().len()}"
                        }
                    }
                }
            }

            div {
                class: "notification-list",
                List {
                    id: "tasks-list-inner",

                    for (i, (group, task)) in tasks.read().iter().enumerate() {
                        // Group header. Mirrors the notification list shell: a plain `<div>`
                        // so the row sits in a normal block-flow container instead of an
                        // anonymous table created by orphan `<thead>/<tr>/<th>` elements.
                        // Anonymous tables shrink-wrap to max-content, which let long task
                        // titles push past the panel width (no truncation despite the
                        // `.ui-nrow-title` ellipsis rule).
                        if current_group != Some(group) {
                            div {
                                class: "flex flex-col px-2 pb-1 text-base-content/50 text-sm border-b snap-start pt-2",
                                span { "{group}" }
                            }
                            { current_group = Some(group); }
                        }

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
        ThirdPartyItemData::SlackReaction(slack_reaction) => rsx! {
            SlackReactionTaskListItem {
                task,
                slack_reaction: *slack_reaction,
                is_selected,
                on_select,
            }
        },
        ThirdPartyItemData::TickTickItem(ticktick_item) => rsx! {
            TickTickTaskListItem {
                task,
                ticktick_item: *ticktick_item,
                is_selected,
                on_select,
            }
        },
        ThirdPartyItemData::SlackThread(_)
        | ThirdPartyItemData::LinearNotification(_)
        | ThirdPartyItemData::GithubNotification(_)
        | ThirdPartyItemData::GoogleMailThread(_)
        | ThirdPartyItemData::GoogleCalendarEvent(_)
        | ThirdPartyItemData::GoogleDriveComment(_)
        | ThirdPartyItemData::WebPage(_) => rsx! {},
    }
}
