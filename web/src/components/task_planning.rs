use log::debug;
use std::collections::HashMap;

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsX, Icon};

use universal_inbox::task::{DueDate, Task, TaskId, TaskPriority};

use crate::{
    components::{
        floating_label_inputs::{
            floating_label_input_date, floating_label_input_text, floating_label_select,
        },
        icons::todoist,
    },
    services::task_service::{TaskPlanningParameters, TaskProject},
};

#[inline_props]
pub fn task_planning_modal<'a>(
    cx: Scope,
    task: Task,
    on_close: EventHandler<'a, ()>,
    on_submit: EventHandler<'a, TaskPlanningParameters>,
) -> Element {
    debug!("Rendering task planning");
    let icon = cx.render(rsx!(self::todoist {}));
    let project = use_state(cx, || "".to_string());
    let due_at = use_state(cx, || "".to_string());
    let priority = use_state(cx, || "".to_string());

    use_memo(cx, &task.clone(), |task| {
        project.set(task.project.clone());
        due_at.set(
            task.due_at
                .as_ref()
                .map(|datetime| match datetime {
                    DueDate::DateTime(dt) => dt.format("%Y-%m-%d").to_string(),
                    DueDate::Date(dt) => dt.format("%Y-%m-%d").to_string(),
                    DueDate::DateTimeWithTz(dt) => dt.format("%Y-%m-%d").to_string(),
                })
                .unwrap_or_default(),
        );
        priority.set((task.priority as i32).to_string());
    });

    let force_validation = use_state(cx, || false);

    cx.render(rsx!(
        dialog {
            id: "task-planning-modal",
            tabindex: "-1",
            class: "text-black dark:text-white backdrop-blur-sm bg-light-0/30 dark:bg-dark-200/30 fixed top-0 left-0 right-0 z-50 w-full p-2 overflow-x-hidden overflow-y-auto md:inset-0 md:h-full flex justify-center items-center",
            open: true,

            div {
                class: "relative w-full h-full max-w-md md:h-auto",

                div {
                    class: "relative bg-light-200 shadow dark:bg-dark-300",

                    button {
                        "type": "button",
                        class: "absolute top-3 right-2.5 px-2 py-1.5 rounded-lg inline-flex hover:shadow-md hover:bg-light-400 hover:dark:bg-dark-600",
                        onclick: move |_| on_close.call(()),
                        tabindex: -1,

                        span { class: "sr-only", "Close" }
                        Icon { class: "w-5 h-5", icon: BsX }
                    }
                    div {
                        class: "px-4 py-4 lg:px-6",

                        h3 {
                            class: "mb-4 text-xl font-medium",
                           "Plan task"
                        }
                        div {
                            class: "flex flex-none items-center gap-2 mb-5",
                            div { class: "h-5 w-5 flex-none", icon }
                            "{task.title}"
                        }

                        form {
                            class: "space-y-4",
                            method: "dialog",
                            onsubmit: |evt| {
                                if let Some(task_planning_parameters) = validate_form(task.id, &evt.data.values) {
                                    on_submit.call(task_planning_parameters);
                                } else {
                                    force_validation.set(true);
                                }
                            },

                            floating_label_input_text::<TaskProject> {
                                name: "task-project-input".to_string(),
                                label: "Project".to_string(),
                                required: true,
                                value: project.clone(),
                                autofocus: true,
                                force_validation: *force_validation.current(),
                            }

                            floating_label_input_date::<DueDate> {
                                name: "task-due_at-input".to_string(),
                                label: "Due at".to_string(),
                                required: false,
                                value: due_at.clone(),
                                force_validation: *force_validation.current(),
                            }

                            floating_label_select::<TaskPriority> {
                                name: "task-priority-input".to_string(),
                                label: "Priority".to_string(),
                                required: false,
                                value: priority.clone(),
                                force_validation: *force_validation.current(),
                            }

                            button {
                                "type": "submit",
                                class: "w-full btn-primary",
                                "Plan"
                            }
                        }
                    }
                }
            }
        }
    ))
}

fn validate_form(
    task_id: TaskId,
    values: &HashMap<String, String>,
) -> Option<TaskPlanningParameters> {
    let project = values["task-project-input"].parse::<TaskProject>();
    let due_at = if values["task-due_at-input"].is_empty() {
        Ok(None)
    } else {
        values["task-due_at-input"].parse::<DueDate>().map(Some)
    };
    let priority = values["task-priority-input"].parse::<TaskPriority>();

    if let (Ok(project), Ok(due_at), Ok(priority)) = (project, due_at, priority) {
        Some(TaskPlanningParameters {
            task_id,
            project,
            due_at,
            priority,
        })
    } else {
        None
    }
}
