#![allow(non_snake_case)]

use std::collections::HashMap;

use chrono::{NaiveDate, Utc};
use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsX, Icon};
use fermi::UseAtomRef;
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
pub fn TaskPlanningModal<'a>(
    cx: Scope,
    notification_to_plan: NotificationWithTask,
    task_service_integration_connection_ref: UseAtomRef<LoadState<Option<IntegrationConnection>>>,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
    on_close: EventHandler<'a, ()>,
    on_task_planning: EventHandler<'a, (TaskPlanning, TaskId)>,
    on_task_creation: EventHandler<'a, TaskCreation>,
) -> Element {
    let icon = render! { div { class: "h-5 w-5 flex-none", Todoist {} } };
    let default_project: &UseState<Option<String>> = use_state(cx, || None);
    let due_at = use_state(cx, || Utc::now().format("%Y-%m-%d").to_string());
    let priority = use_state(cx, || Some(TaskPriority::P4));
    let task_title = use_state(cx, || "".to_string());
    let task_to_plan = use_state(cx, || None);

    let force_validation = use_state(cx, || false);

    let selected_project: &UseState<Option<ProjectSummary>> = use_state(cx, || None);

    let _ = use_memo(cx, &notification_to_plan.clone(), |notification| {
        if let Some(task) = notification.task {
            task_title.set(task.title.clone());
            default_project.set(Some(task.project.clone()));
            if let Some(task_due_at) = task.due_at.as_ref() {
                due_at.set(match task_due_at {
                    DueDate::DateTime(dt) => dt.format("%Y-%m-%d").to_string(),
                    DueDate::Date(dt) => dt.format("%Y-%m-%d").to_string(),
                    DueDate::DateTimeWithTz(dt) => dt.format("%Y-%m-%d").to_string(),
                });
            }
            priority.set(Some(task.priority));
            task_to_plan.set(Some(task));
        } else {
            task_to_plan.set(None);
            task_title.set(notification.title);

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
            })) = *task_service_integration_connection_ref.read()
            {
                if !create_notification_from_inbox_task {
                    default_project.set(Some(TODOIST_INBOX_PROJECT.to_string()));
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
    })) = *task_service_integration_connection_ref.read()
    {
        create_notification_from_inbox_task
    } else {
        false
    };

    render! {
        dialog {
            id: "task-planning-modal",
            tabindex: "-1",
            class: "modal modal-open text-base-content backdrop-blur-sm fixed top-0 left-0 w-full h-full z-50",
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
                        onsubmit: |evt| {
                            if let Some(ref task) = *task_to_plan.current() {
                                if let Some(task_planning_parameters) = validate_planning_form(
                                    &evt.data.values, &selected_project.current()
                                ) {
                                    on_task_planning.call((task_planning_parameters, task.id));
                                }
                            } else if let Some(task_creation_parameters) = validate_creation_form(
                                &evt.data.values, &selected_project.current()
                            ) {
                                on_task_creation.call(task_creation_parameters);
                            }
                        },

                        div {
                            class: "flex flex-none items-center gap-2 w-full",
                            if task_to_plan.current().is_some() {
                                render! {
                                    icon
                                    span { class: "grow", "{task_title}" }
                                }
                            } else {
                                render! {
                                    FloatingLabelInputText::<String> {
                                        name: "task-title-input".to_string(),
                                        label: Some("Task's title"),
                                        required: true,
                                        value: task_title.clone(),
                                        autofocus: true,
                                        force_validation: *force_validation.current(),
                                        icon: icon,
                                    }
                                }
                            }
                        }

                        TaskProjectSearch {
                            label: "Project",
                            required: true,
                            selected_project: selected_project.clone(),
                            ui_model_ref: ui_model_ref.clone(),
                            filter_out_inbox: filter_out_inbox,
                            on_select: move |_project| {
                                cx.spawn({
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
                            value: due_at.clone(),
                            force_validation: *force_validation.current(),
                            autohide: true,
                            today_button: true,
                            today_highlight: true,
                        }

                        FloatingLabelSelect::<TaskPriority> {
                            name: "task-priority-input".to_string(),
                            label: Some("Priority"),
                            required: false,
                            value: priority.clone(),
                            force_validation: *force_validation.current(),

                            option { value: "1", "🔴 Priority 1" }
                            option { value: "2", "🟠 Priority 2" }
                            option { value: "3", "🟡 Priority 3" }
                            option { value: "4", "🔵 Priority 4" }
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
    values: &HashMap<String, Vec<String>>,
    selected_project: &Option<ProjectSummary>,
) -> Option<TaskPlanning> {
    let due_at = values["task-due_at-input"]
        .first()
        .map_or(Ok(None), |value| {
            if value.is_empty() {
                Ok(None)
            } else {
                value.parse::<DueDate>().map(Some)
            }
        });
    let priority = values["task-priority-input"].first().map_or(
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
    values: &HashMap<String, Vec<String>>,
    selected_project: &Option<ProjectSummary>,
) -> Option<TaskCreation> {
    if let Some(task_planning_parameters) = validate_planning_form(values, selected_project) {
        let title = values["task-title-input"]
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
