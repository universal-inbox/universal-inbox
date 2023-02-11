use log::debug;
use std::collections::HashMap;

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsX, Icon};

use universal_inbox::{
    notification::NotificationWithTask,
    task::{DueDate, TaskCreation, TaskId, TaskPlanning, TaskPriority, TaskProject},
};

use crate::components::{
    floating_label_inputs::{
        floating_label_input_date, floating_label_input_text, floating_label_select,
    },
    icons::todoist,
};

#[inline_props]
pub fn task_planning_modal<'a>(
    cx: Scope,
    notification_to_plan: NotificationWithTask,
    on_close: EventHandler<'a, ()>,
    on_task_planning: EventHandler<'a, (TaskPlanning, TaskId)>,
    on_task_creation: EventHandler<'a, TaskCreation>,
) -> Element {
    debug!("Rendering task planning");
    let icon = cx.render(rsx!(self::todoist {}));
    let project = use_state(cx, || "".to_string());
    let due_at = use_state(cx, || "".to_string()); // TODO Set today as default
    let priority = use_state(cx, || "4".to_string());
    let task_title = use_state(cx, || "".to_string());
    let task_to_plan = use_state(cx, || None);

    use_memo(cx, &notification_to_plan.clone(), |notification| {
        if let Some(task) = notification.task {
            task_title.set(task.title.clone());
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
            task_to_plan.set(Some(task));
        } else {
            task_to_plan.set(None);
            task_title.set(notification.title);
        }
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

                        form {
                            class: "space-y-4",
                            method: "dialog",
                            onsubmit: |evt| {
                                if let Some(ref task) = *task_to_plan.current() {
                                    if let Some(task_planning_parameters) = validate_planning_form(&evt.data.values) {
                                        on_task_planning.call((task_planning_parameters, task.id));
                                    }
                                } else if let Some(task_creation_parameters) = validate_creation_form(&evt.data.values) {
                                    on_task_creation.call(task_creation_parameters);
                                }
                            },

                            div {
                                class: "flex flex-none items-center gap-2 mb-5",
                                div { class: "h-5 w-5 flex-none", icon }
                                (task_to_plan.current().is_some()).then(|| rsx!(
                                    "{task_title}"
                                )),
                                (task_to_plan.current().is_none()).then(|| rsx!(
                                    floating_label_input_text::<String> {
                                        name: "task-title-input".to_string(),
                                        label: "Task's title".to_string(),
                                        required: true,
                                        value: task_title.clone(),
                                        autofocus: true,
                                        force_validation: *force_validation.current(),
                                    }
                                ))
                            },


                            floating_label_input_text::<TaskProject> {
                                name: "task-project-input".to_string(),
                                label: "Project".to_string(),
                                required: true,
                                value: project.clone(),
                                autofocus: task_to_plan.current().is_some(),
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

fn validate_planning_form(values: &HashMap<String, String>) -> Option<TaskPlanning> {
    let project = values["task-project-input"].parse::<TaskProject>();
    let due_at = if values["task-due_at-input"].is_empty() {
        Ok(None)
    } else {
        values["task-due_at-input"].parse::<DueDate>().map(Some)
    };
    let priority = values["task-priority-input"].parse::<TaskPriority>();

    if let (Ok(project), Ok(due_at), Ok(priority)) = (project, due_at, priority) {
        return Some(TaskPlanning {
            project,
            due_at,
            priority,
        });
    }

    None
}

fn validate_creation_form(values: &HashMap<String, String>) -> Option<TaskCreation> {
    if let Some(task_planning_parameters) = validate_planning_form(values) {
        let title = values["task-title-input"].parse::<String>();

        if let Ok(title) = title {
            return Some(TaskCreation {
                title,
                body: None,
                project: task_planning_parameters.project,
                due_at: task_planning_parameters.due_at,
                priority: task_planning_parameters.priority,
            });
        }
    }

    None
}
