#![allow(non_snake_case)]

use std::collections::HashMap;

use chrono::{NaiveDate, Utc};
use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsX, Icon};

use log::error;

use universal_inbox::{
    integration_connection::{
        integrations::todoist::TodoistConfig, provider::IntegrationProvider, IntegrationConnection,
    },
    notification::NotificationWithTask,
    task::{
        integrations::todoist::TODOIST_INBOX_PROJECT, DueDate, ProjectSummary, TaskCreation,
        TaskId, TaskPlanning, TaskPriority,
    },
};

use crate::{
    components::{
        floating_label_inputs::{FloatingLabelInputText, FloatingLabelSelect},
        flowbite::datepicker::DatePicker,
        integrations::{task_project_search::TaskProjectSearch, todoist::icons::Todoist},
    },
    model::{LoadState, UniversalInboxUIModel},
    utils::focus_element,
};

#[component]
pub fn TaskPlanningModal(
    notification_to_plan: ReadOnlySignal<NotificationWithTask>,
    task_service_integration_connection: Signal<LoadState<Option<IntegrationConnection>>>,
    ui_model: Signal<UniversalInboxUIModel>,
    on_close: EventHandler<()>,
    on_task_planning: EventHandler<(TaskPlanning, TaskId)>,
    on_task_creation: EventHandler<TaskCreation>,
) -> Element {
    let icon = rsx! { div { class: "h-5 w-5 flex-none", Todoist {} } };
    let mut default_project: Signal<Option<String>> = use_signal(|| None);
    let mut due_at = use_signal(|| Utc::now().format("%Y-%m-%d").to_string());
    let mut priority = use_signal(|| Some(TaskPriority::P4));
    let mut task_title = use_signal(|| "".to_string());
    let mut task_to_plan = use_signal(|| None);

    let force_validation = use_signal(|| false);

    let selected_project: Signal<Option<ProjectSummary>> = use_signal(|| None);

    let _ = use_memo(move || {
        if let Some(task) = notification_to_plan().task {
            task_title.write().clone_from(&task.title);
            *default_project.write() = Some(task.project.clone());
            if let Some(task_due_at) = task.due_at.as_ref() {
                *due_at.write() = match task_due_at {
                    DueDate::DateTime(dt) => dt.format("%Y-%m-%d").to_string(),
                    DueDate::Date(dt) => dt.format("%Y-%m-%d").to_string(),
                    DueDate::DateTimeWithTz(dt) => dt.format("%Y-%m-%d").to_string(),
                };
            }
            *priority.write() = Some(task.priority);
            *task_to_plan.write() = Some(task);
        } else {
            *task_to_plan.write() = None;
            *task_title.write() = notification_to_plan().title;

            if let LoadState::Loaded(Some(IntegrationConnection {
                provider:
                    IntegrationProvider::Todoist {
                        config:
                            TodoistConfig {
                                create_notification_from_inbox_task,
                                ..
                            },
                        ..
                    },
                ..
            })) = *task_service_integration_connection.read()
            {
                if !create_notification_from_inbox_task {
                    *default_project.write() = Some(TODOIST_INBOX_PROJECT.to_string());
                }
            }
        }
    });

    let filter_out_inbox = if let LoadState::Loaded(Some(IntegrationConnection {
        provider:
            IntegrationProvider::Todoist {
                config:
                    TodoistConfig {
                        create_notification_from_inbox_task,
                        ..
                    },
                ..
            },
        ..
    })) = *task_service_integration_connection.read()
    {
        create_notification_from_inbox_task
    } else {
        false
    };

    rsx! {
        dialog {
            id: "task-planning-modal",
            tabindex: "-1",
            class: "modal modal-open text-base-content backdrop-blur-xs fixed top-0 left-0 w-full h-full z-50",
            open: true,

            div {
                class: "modal-box relative w-96 overflow-x-hidden overflow-y-hidden",

                button {
                    "type": "button",
                    class: "btn btn-sm btn-ghost absolute right-2 top-5",
                    onclick: move |_| on_close.call(()),
                    tabindex: -1,

                    span { class: "sr-only", "Close" }
                    Icon { class: "w-5 h-5", icon: BsX }
                }
                div {
                    h3 {
                        class: "mb-4 text-xl font-medium",
                        "Plan task"
                    }

                    form {
                        class: "flex flex-col space-y-4",
                        method: "dialog",
                        onsubmit: move |evt| {
                            if let Some(task) = task_to_plan() {
                                if let Some(task_planning_parameters) = validate_planning_form(
                                    &evt.data.values(), selected_project()
                                ) {
                                    on_task_planning.call((task_planning_parameters, task.id));
                                }
                            } else if let Some(task_creation_parameters) = validate_creation_form(
                                &evt.data.values(), selected_project()
                            ) {
                                on_task_creation.call(task_creation_parameters);
                            }
                        },

                        div {
                            class: "flex flex-none items-center gap-2 w-full",
                            if task_to_plan().is_some() {
                                { icon }
                                span { class: "grow", "{task_title}" }
                            } else {
                                FloatingLabelInputText::<String> {
                                    name: "task-title-input".to_string(),
                                    label: Some("Task's title".to_string()),
                                    required: true,
                                    value: task_title,
                                    autofocus: true,
                                    force_validation: force_validation(),
                                    icon: icon,
                                }
                            }
                        }

                        TaskProjectSearch {
                            label: "Project",
                            required: true,
                            selected_project: selected_project,
                            default_project_name: None,
                            ui_model: ui_model,
                            filter_out_inbox: filter_out_inbox,
                            on_select: move |_project| {
                                spawn({
                                    async move {
                                        if let Err(error) = focus_element("task-planning-modal-submit").await {
                                            error!("Error focusing element task-planning-modal-submit: {error:?}");
                                        }
                                    }
                                });
                            },
                        }

                        DatePicker::<NaiveDate> {
                            name: "task-due_at-input".to_string(),
                            label: "Due at",
                            required: false,
                            value: due_at,
                            force_validation: force_validation(),
                            autohide: true,
                            today_button: true,
                            today_highlight: true,
                        }

                        FloatingLabelSelect::<TaskPriority> {
                            name: "task-priority-input".to_string(),
                            label: Some("Priority".to_string()),
                            required: false,
                            force_validation: force_validation(),

                            option { selected: priority() == Some(TaskPriority::P1), value: "1", "🔴 Priority 1" }
                            option { selected: priority() == Some(TaskPriority::P2), value: "2", "🟠 Priority 2" }
                            option { selected: priority() == Some(TaskPriority::P3), value: "3", "🟡 Priority 3" }
                            option { selected: priority() == Some(TaskPriority::P4), value: "4", "🔵 Priority 4" }
                        }

                        div {
                            class: "modal-action",
                            button {
                                id: "task-planning-modal-submit",
                                "type": "submit",
                                class: "btn btn-primary w-full",
                                "Plan"
                            }
                        }
                    }
                }
            }
        }
    }
}

fn validate_planning_form(
    values: &HashMap<String, FormValue>,
    selected_project: Option<ProjectSummary>,
) -> Option<TaskPlanning> {
    let due_at = values["task-due_at-input"]
        .clone()
        .to_vec()
        .first()
        .map_or(Ok(None), |value| {
            if value.is_empty() {
                Ok(None)
            } else {
                value.parse::<DueDate>().map(Some)
            }
        });
    let priority = values["task-priority-input"]
        .clone()
        .to_vec()
        .first()
        .map_or(
            Err("Task priority value is required".to_string()),
            |value| value.parse::<TaskPriority>(),
        );

    if let (Some(project), Ok(due_at), Ok(priority)) = (selected_project, due_at, priority) {
        return Some(TaskPlanning {
            project: project.clone(),
            due_at,
            priority,
        });
    }

    None
}

fn validate_creation_form(
    values: &HashMap<String, FormValue>,
    selected_project: Option<ProjectSummary>,
) -> Option<TaskCreation> {
    if let Some(task_planning_parameters) = validate_planning_form(values, selected_project) {
        let title_input = values["task-title-input"].clone().to_vec();
        let title = title_input
            .first()
            .ok_or("Task title is required".to_string());

        if let Ok(title) = title {
            return Some(TaskCreation {
                title: title.to_string(),
                body: None,
                project: task_planning_parameters.project,
                due_at: task_planning_parameters.due_at,
                priority: task_planning_parameters.priority,
            });
        }
    }

    None
}
