#![allow(non_snake_case)]

use std::cmp::Ordering;

#[cfg(feature = "web")]
use dioxus::prelude::dioxus_core::use_drop;
use dioxus::prelude::*;
use log::debug;
use sorted_groups::SortedGroups;

#[cfg(feature = "web")]
use universal_inbox::HasHtmlUrl;
use universal_inbox::{
    Page,
    task::{Task, TaskId},
};

use crate::{
    components::{task_preview::TaskPreview, tasks_list::TasksList, welcome_hero::WelcomeHero},
    model::UI_MODEL,
    route::Route,
    services::task_service::{SYNCED_TASKS_PAGE, TaskCommand},
    settings::PanelPosition,
};

#[cfg(feature = "web")]
use crate::{
    components::resizable_panel::ResizablePanel,
    keyboard_manager::{KEYBOARD_MANAGER, KeyboardHandler},
    utils::{
        get_screen_width, open_link, scroll_element, scroll_element_by_page,
        scroll_element_into_view_by_class,
    },
};

#[cfg(not(feature = "web"))]
fn get_screen_width() -> Result<usize, anyhow::Error> {
    Ok(0)
}

#[cfg(feature = "web")]
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
pub fn SyncedTaskPage(task_id: TaskId) -> Element {
    rsx! { InternalSyncedTaskPage { task_id } }
}

#[component]
pub fn SyncedTasksPage() -> Element {
    use_effect(move || {
        let tasks_count = SYNCED_TASKS_PAGE().content.len();
        if tasks_count > 0 {
            let mut model = UI_MODEL.write();
            if let Some(index) = model.selected_task_index {
                if index >= tasks_count {
                    model.selected_task_index = Some(tasks_count - 1);
                }
            } else if get_screen_width().unwrap_or_default() >= 1024 {
                // ie. lg screen
                model.selected_task_index = Some(0);
            }
        }
    });

    rsx! { InternalSyncedTaskPage {} }
}

#[component]
fn InternalSyncedTaskPage(task_id: ReadSignal<Option<TaskId>>) -> Element {
    let tasks = Into::<ReadSignal<Page<Task>>>::into(SYNCED_TASKS_PAGE.signal());
    let nav = use_navigator();
    debug!("Rendering synced tasks page for task {:?}", task_id(),);

    use_effect(move || {
        *SORTED_SYNCED_TASKS.write() = SortedGroups::new(
            SYNCED_TASKS_PAGE()
                .content
                .into_iter()
                .map(|task| TaskWithOrder {
                    task,
                    compare_by: CompareBy::Priority,
                }),
            due_at_group_from_task,
        );
    });

    use_effect(move || {
        if let Some(task_id) = task_id() {
            if let Some(task_index) = SORTED_SYNCED_TASKS()
                .iter()
                .position(|(_, t)| t.task.id == task_id)
                && UI_MODEL.peek().selected_task_index != Some(task_index)
            {
                UI_MODEL.write().selected_task_index = Some(task_index);
            }
        } else if UI_MODEL.peek().selected_task_index.is_some()
            && get_screen_width().unwrap_or_default() < 1024
        {
            UI_MODEL.write().selected_task_index = None;
        }
    });

    use_effect(move || {
        if let Some(index) = UI_MODEL.read().selected_task_index {
            if let Some((_, selected_task)) = SORTED_SYNCED_TASKS().get(index)
                && *task_id.peek() != Some(selected_task.task.id)
            {
                let route = Route::SyncedTaskPage {
                    task_id: selected_task.task.id,
                };
                nav.push(route);
            }
        } else if task_id.peek().is_some() {
            nav.push(Route::SyncedTasksPage {});
        }
    });

    #[cfg(feature = "web")]
    use_drop(move || {
        KEYBOARD_MANAGER.write().active_keyboard_handler = None;
    });

    let panel_position = UI_MODEL.read().get_details_panel_position().clone();
    let layout_class = match panel_position {
        PanelPosition::Right => {
            "h-full mx-auto flex flex-row lg:px-4 lg:divide-x divide-base-content/25 relative"
        }
        PanelPosition::Bottom => {
            "h-full mx-auto flex flex-col lg:px-4 lg:divide-y divide-base-content/25 relative"
        }
    };

    rsx! {
        div {
            id: "tasks-page",
            class: "{layout_class}",
            onmounted: move |_| {
                #[cfg(feature = "web")]
                {
                    KEYBOARD_MANAGER.write().active_keyboard_handler = Some(&KEYBOARD_HANDLER);
                }
            },

            if SORTED_SYNCED_TASKS().is_empty() {
                WelcomeHero { inbox_zero_message: "Your synchronized tasks will appear here when they arrive." }
            } else {
                div {
                    class: match panel_position {
                        PanelPosition::Right => "h-full flex-1 max-lg:w-full max-lg:absolute",
                        PanelPosition::Bottom => "flex-1 max-lg:w-full max-lg:absolute overflow-y-auto",
                    },

                    TasksList { tasks: SORTED_SYNCED_TASKS.signal() }
                }

                if let Some(index) = UI_MODEL.read().selected_task_index {
                    if let Some((_, task)) = SORTED_SYNCED_TASKS().get(index) {
                        { task_detail_panel(task.task.clone(), tasks().content.len()) }
                    }
                }
            }
        }
    }
}

#[cfg(feature = "web")]
fn task_detail_panel(task: Task, tasks_count: usize) -> Element {
    rsx! {
        ResizablePanel {
            TaskPreview {
                task: task,
                expand_details: UI_MODEL.read().preview_cards_expanded,
                is_help_enabled: UI_MODEL.read().is_help_enabled,
                ui_model: UI_MODEL.signal(),
                tasks_count: tasks_count
            }
        }
    }
}

#[cfg(not(feature = "web"))]
fn task_detail_panel(task: Task, tasks_count: usize) -> Element {
    rsx! {
        div {
            class: "h-full flex-1 overflow-y-auto",
            TaskPreview {
                task: task,
                expand_details: UI_MODEL.read().preview_cards_expanded,
                is_help_enabled: UI_MODEL.read().is_help_enabled,
                ui_model: UI_MODEL.signal(),
                tasks_count: tasks_count
            }
        }
    }
}

#[cfg(feature = "web")]
#[derive(PartialEq)]
struct SyncTasksPageKeyboardHandler {}

#[cfg(feature = "web")]
impl KeyboardHandler for SyncTasksPageKeyboardHandler {
    fn handle_keydown(&self, event: &web_sys::KeyboardEvent) -> bool {
        let task_service = use_coroutine_handle::<TaskCommand>();
        let sorted_tasks = SORTED_SYNCED_TASKS();
        let list_length = sorted_tasks.len();
        let selected_task_index = UI_MODEL.peek().selected_task_index;
        let selected_task = selected_task_index.and_then(|index| sorted_tasks.get(index));
        let mut handled = true;

        match (
            event.key().as_ref(),
            event.ctrl_key(),
            event.meta_key(),
            event.alt_key(),
            event.shift_key(),
        ) {
            ("ArrowDown", false, false, false, false) => {
                if let Some(index) = selected_task_index
                    && index < (list_length - 1)
                {
                    let new_index = index + 1;
                    let mut ui_model = UI_MODEL.write();
                    ui_model.selected_task_index = Some(new_index);
                    drop(ui_model);
                    let _ = scroll_element_into_view_by_class("tasks_list", "row-hover", new_index);
                }
            }
            ("ArrowUp", false, false, false, false) => {
                if let Some(index) = selected_task_index
                    && index > 0
                {
                    let new_index = index - 1;
                    let mut ui_model = UI_MODEL.write();
                    ui_model.selected_task_index = Some(new_index);
                    drop(ui_model);
                    let _ = scroll_element_into_view_by_class("tasks_list", "row-hover", new_index);
                }
            }
            ("c", false, false, false, false) => {
                if let Some((_, TaskWithOrder { task, .. })) = selected_task {
                    task_service.send(TaskCommand::Complete(task.id))
                }
            }
            ("j", false, false, false, false) => {
                let _ = scroll_element("task-preview", 100.0);
            }
            ("k", false, false, false, false) => {
                let _ = scroll_element("task-preview", -100.0);
            }
            (" ", false, false, false, false) => {
                let _ = scroll_element_by_page("task-preview");
            }
            ("e", false, false, false, false) => {
                UI_MODEL.write().toggle_preview_cards();
            }
            ("Enter", false, false, false, false) => {
                if let Some((_, TaskWithOrder { task, .. })) = selected_task {
                    let _ = open_link(task.get_html_url().as_str());
                }
            }
            ("h", false, false, false, false) | ("?", false, false, false, false) => {
                UI_MODEL.write().toggle_help()
            }
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
    #[allow(dead_code)]
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
