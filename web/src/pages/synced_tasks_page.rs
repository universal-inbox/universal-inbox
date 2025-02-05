#![allow(non_snake_case)]

use std::cmp::Ordering;

use dioxus::prelude::*;
use log::debug;
use sorted_groups::SortedGroups;
use web_sys::KeyboardEvent;

use universal_inbox::{task::Task, HasHtmlUrl};

use crate::{
    components::{task_preview::TaskPreview, tasks_list::TasksList},
    images::UI_LOGO_SYMBOL_TRANSPARENT,
    keyboard_manager::{KeyboardHandler, KEYBOARD_MANAGER},
    model::UI_MODEL,
    services::task_service::{TaskCommand, SYNCED_TASKS_PAGE},
    utils::{open_link, scroll_element, scroll_element_by_page},
};

static KEYBOARD_HANDLER: SyncTasksPageKeyboardHandler = SyncTasksPageKeyboardHandler {};

fn due_at_group_from_task(task: &TaskWithOrder) -> String {
    task.task
        .due_at
        .as_ref()
        .map(|due_at| due_at.display_date())
        .unwrap_or_else(|| "No due date".to_string())
}

static SORTED_SYNCED_TASKS: GlobalSignal<SortedGroups<String, TaskWithOrder>> =
    Signal::global(|| SortedGroups::new(vec![], due_at_group_from_task));

#[component]
pub fn SyncedTasksPage() -> Element {
    debug!("Rendering synced tasks page");

    use_effect(move || {
        let tasks_count = SYNCED_TASKS_PAGE().content.len();
        if tasks_count > 0 && UI_MODEL.read().selected_task_index >= tasks_count {
            UI_MODEL.write().selected_task_index = tasks_count - 1;
        }
        *SORTED_SYNCED_TASKS.write() = SortedGroups::new(
            SYNCED_TASKS_PAGE()
                .content
                .into_iter()
                .map(|task| TaskWithOrder {
                    task,
                    compare_by: CompareBy::DueAt,
                }),
            due_at_group_from_task,
        );
    });

    use_drop(move || {
        KEYBOARD_MANAGER.write().active_keyboard_handler = None;
    });

    rsx! {
        div {
            id: "tasks-page",
            class: "h-full mx-auto flex flex-row px-4 divide-x divide-base-200",
            onmounted: move |_| {
                KEYBOARD_MANAGER.write().active_keyboard_handler = Some(&KEYBOARD_HANDLER);
            },

            if SORTED_SYNCED_TASKS().is_empty() {
                div {
                    class: "relative w-full h-full flex justify-center items-center",
                    img {
                        class: "h-full opacity-30 dark:opacity-10",
                        src: "{UI_LOGO_SYMBOL_TRANSPARENT}",
                        alt: "No synchronized tasks"
                    }
                    div {
                        class: "flex flex-col items-center absolute object-center top-2/3 transform translate-y-1/4",
                        p { class: "text-gray-500 font-semibold", "Congrats! You have completed all synchronized tasks 🎉" }
                    }
                }
            } else {
                div {
                    id: "synced-tasks-list",
                    class: "h-full basis-2/3 overflow-auto scroll-auto px-2 snap-y snap-mandatory",

                    TasksList { tasks: SORTED_SYNCED_TASKS.signal() }
                }

                if let Some((_, task)) = SORTED_SYNCED_TASKS().get(UI_MODEL.read().selected_task_index) {
                    div {
                        id: "task-preview",
                        class: "h-full basis-1/3 overflow-auto scroll-auto px-2 py-2 flex flex-row",

                        TaskPreview {
                            task: task.task.clone(),
                            expand_details: UI_MODEL.read().preview_cards_expanded,
                            is_help_enabled: UI_MODEL.read().is_help_enabled,
                        }
                    }
                }
            }
        }
    }
}

#[derive(PartialEq)]
struct SyncTasksPageKeyboardHandler {}

impl KeyboardHandler for SyncTasksPageKeyboardHandler {
    fn handle_keydown(&self, event: &KeyboardEvent) -> bool {
        let task_service = use_coroutine_handle::<TaskCommand>();
        let sorted_tasks = SORTED_SYNCED_TASKS();
        let list_length = sorted_tasks.len();
        let selected_task = sorted_tasks.get(UI_MODEL.peek().selected_task_index);
        let mut handled = true;

        match event.key().as_ref() {
            "ArrowDown" if UI_MODEL.peek().selected_task_index < (list_length - 1) => {
                let mut ui_model = UI_MODEL.write();
                ui_model.selected_task_index += 1;
            }
            "ArrowUp" if UI_MODEL.peek().selected_task_index > 0 => {
                let mut ui_model = UI_MODEL.write();
                ui_model.selected_task_index -= 1;
            }
            "c" => {
                if let Some((_, TaskWithOrder { task, .. })) = selected_task {
                    task_service.send(TaskCommand::Complete(task.id))
                }
            }
            "j" => {
                let _ = scroll_element("task-preview", 100.0);
            }
            "k" => {
                let _ = scroll_element("task-preview", -100.0);
            }
            " " => {
                let _ = scroll_element_by_page("task-preview");
            }
            "e" => {
                UI_MODEL.write().toggle_preview_cards();
            }
            "Enter" => {
                if let Some((_, TaskWithOrder { task, .. })) = selected_task {
                    let _ = open_link(task.get_html_url().as_str());
                }
            }
            "h" | "?" => UI_MODEL.write().toggle_help(),
            _ => handled = false,
        }

        handled
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct TaskWithOrder {
    pub task: Task,
    compare_by: CompareBy,
}

impl Eq for TaskWithOrder {}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
enum CompareBy {
    #[allow(dead_code)]
    Priority,
    DueAt,
}

impl PartialOrd for TaskWithOrder {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TaskWithOrder {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.compare_by {
            CompareBy::Priority => {
                let ordering = self.task.priority.cmp(&other.task.priority);
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            CompareBy::DueAt => {
                let Some(due_at) = &self.task.due_at else {
                    return Ordering::Less;
                };
                let Some(other_due_at) = &other.task.due_at else {
                    return Ordering::Greater;
                };

                let ordering = due_at.display_date().cmp(&other_due_at.display_date());
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
        }

        if self.task.eq(&other.task) {
            return Ordering::Equal;
        }

        Ordering::Less
    }
}
