#![allow(non_snake_case)]

use std::collections::HashMap;

use anyhow::Result;
use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsX, Icon};
use fermi::UseAtomRef;
use gloo_timers::future::TimeoutFuture;
use http::Method;
use log::error;
use url::Url;

use universal_inbox::{
    notification::NotificationWithTask,
    task::{DueDate, ProjectSummary, TaskCreation, TaskId, TaskPlanning, TaskPriority},
};

use crate::{
    components::{
        floating_label_inputs::{
            FloatingLabelInputDate, FloatingLabelInputSearchSelect, FloatingLabelInputText,
            FloatingLabelSelect, Searchable,
        },
        icons::Todoist,
    },
    model::UniversalInboxUIModel,
    services::api::call_api,
    utils::focus_element,
};

#[component]
pub fn TaskPlanningModal<'a>(
    cx: Scope,
    api_base_url: Url,
    notification_to_plan: NotificationWithTask,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
    on_close: EventHandler<'a, ()>,
    on_task_planning: EventHandler<'a, (TaskPlanning, TaskId)>,
    on_task_creation: EventHandler<'a, TaskCreation>,
) -> Element {
    let icon = render! { Todoist {} };
    let project = use_state(cx, || "".to_string());
    let due_at = use_state(cx, || "".to_string()); // TODO Set today as default
    let priority = use_state(cx, || "4".to_string());
    let task_title = use_state(cx, || "".to_string());
    let task_to_plan = use_state(cx, || None);

    let _ = use_memo(cx, &notification_to_plan.clone(), |notification| {
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

    let selected_project: &UseState<Option<ProjectSummary>> = use_state(cx, || None);
    let search_expression = use_state(cx, || "".to_string());
    let search_results: &UseState<Vec<ProjectSummary>> = use_state(cx, Vec::new);

    use_future(cx, &search_expression.clone(), |search_expression| {
        to_owned![search_results];
        to_owned![api_base_url];
        to_owned![ui_model_ref];

        async move {
            TimeoutFuture::new(500).await;
            search_results.set(
                search_projects(&api_base_url, &search_expression.current(), ui_model_ref).await,
            );
        }
    });

    render! {
        dialog {
            id: "task-planning-modal",
            tabindex: "-1",
            class: "modal modal-open text-base-content backdrop-blur-sm fixed top-0 left-0 w-full h-full",
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
                            class: "flex flex-none items-center gap-2",
                            div { class: "h-5 w-5 flex-none", icon }
                            div {
                                class: "grow",
                                if task_to_plan.current().is_some() {
                                    render! { "{task_title}" }
                                } else {
                                    render! {
                                        FloatingLabelInputText::<String> {
                                            name: "task-title-input".to_string(),
                                            label: "Task's title".to_string(),
                                            required: true,
                                            value: task_title.clone(),
                                            autofocus: true,
                                            force_validation: *force_validation.current(),
                                        }
                                    }
                                }
                            }
                        }

                        FloatingLabelInputSearchSelect {
                            name: "project-search-input".to_string(),
                            label: "Project".to_string(),
                            required: true,
                            value: selected_project.clone(),
                            search_expression: search_expression.clone(),
                            search_results: search_results.clone(),
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

                        FloatingLabelInputDate::<DueDate> {
                            name: "task-due_at-input".to_string(),
                            label: "Due at".to_string(),
                            required: false,
                            value: due_at.clone(),
                            force_validation: *force_validation.current(),
                        }

                        FloatingLabelSelect::<TaskPriority> {
                            name: "task-priority-input".to_string(),
                            label: "Priority".to_string(),
                            required: false,
                            value: priority.clone(),
                            force_validation: *force_validation.current(),
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

async fn search_projects(
    api_base_url: &Url,
    search: &str,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
) -> Vec<ProjectSummary> {
    let search_result: Result<Vec<ProjectSummary>> = call_api(
        Method::GET,
        api_base_url,
        &format!("tasks/projects/search?matches={search}"),
        None::<i32>,
        Some(ui_model_ref),
    )
    .await;

    match search_result {
        Ok(projects) => projects.into_iter().filter(|p| p.name != "Inbox").collect(),
        Err(error) => {
            error!("Error searching projects: {error:?}");
            Vec::new()
        }
    }
}

impl Searchable for ProjectSummary {
    fn get_title(&self) -> String {
        self.name.clone()
    }

    fn get_id(&self) -> String {
        self.source_id.clone()
    }
}
