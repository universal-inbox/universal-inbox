#![allow(non_snake_case)]

use dioxus::prelude::*;
use log::debug;
use web_sys::KeyboardEvent;

use universal_inbox::HasHtmlUrl;

use crate::{
    components::{task_preview::TaskPreview, tasks_list::TasksList},
    keyboard_manager::{KeyboardHandler, KEYBOARD_MANAGER},
    model::UI_MODEL,
    services::task_service::{TaskCommand, SYNCED_TASKS_PAGE},
    utils::{open_link, scroll_element, scroll_element_by_page},
};

static KEYBOARD_HANDLER: SyncTasksPageKeyboardHandler = SyncTasksPageKeyboardHandler {};

#[component]
pub fn SyncedTasksPage() -> Element {
    debug!("Rendering synced tasks page");

    use_effect(move || {
        let tasks_count = SYNCED_TASKS_PAGE().content.len();
        if tasks_count > 0 && UI_MODEL.read().selected_task_index >= tasks_count {
            UI_MODEL.write().selected_task_index = tasks_count - 1;
        }
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

            if SYNCED_TASKS_PAGE.read().content.is_empty() {
                div {
                    class: "relative w-full h-full flex justify-center items-center",
                    img {
                        class: "h-full opacity-30 dark:opacity-10",
                        src: "images/ui-logo-symbol-transparent.svg",
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

                    TasksList {
                        tasks: SYNCED_TASKS_PAGE.read().content.clone(),
                    }
                }

                if let Some(task) = SYNCED_TASKS_PAGE().content
                    .get(UI_MODEL.read().selected_task_index) {
                    div {
                        id: "task-preview",
                        class: "h-full basis-1/3 overflow-auto scroll-auto px-2 py-2 flex flex-row",

                        TaskPreview {
                            task: task.clone(),
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
        let tasks_page = SYNCED_TASKS_PAGE();
        let list_length = tasks_page.content.len();
        let selected_task = tasks_page.content.get(UI_MODEL.peek().selected_task_index);
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
                if let Some(task) = selected_task {
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
                if let Some(task) = selected_task {
                    let _ = open_link(task.get_html_url().as_str());
                }
            }
            "h" | "?" => UI_MODEL.write().toggle_help(),
            _ => handled = false,
        }

        handled
    }
}
