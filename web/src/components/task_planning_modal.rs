#![allow(non_snake_case)]

use chrono::{NaiveDate, Utc};
use dioxus::prelude::dioxus_core::use_drop;
use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use log::error;
use serde_json::json;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionId, integrations::todoist::TodoistConfig,
        provider::IntegrationProvider,
    },
    notification::{NotificationId, NotificationWithTask},
    task::{
        DueDate, ProjectSummary, TaskCreation, TaskId, TaskPlanning, TaskPriority,
        integrations::todoist::TODOIST_INBOX_PROJECT,
    },
};
use url::Url;

use crate::{
    components::{
        datepicker::DatePicker,
        floating_label_inputs::{
            FloatingLabelInputSearchSelect, FloatingLabelInputText, FloatingLabelSelect,
        },
        integrations::todoist::icons::Todoist,
    },
    model::{LoadState, UniversalInboxUIModel},
    services::flyonui::{close_flyonui_modal, forget_flyonui_modal, init_flyonui_modal},
    utils::focus_element,
};

#[component]
pub fn TaskPlanningModal(
    api_base_url: Url,
    notification_to_plan: ReadSignal<NotificationWithTask>,
    task_service_integration_connection: Signal<LoadState<Option<IntegrationConnection>>>,
    ui_model: Signal<UniversalInboxUIModel>,
    on_task_planning: EventHandler<(TaskPlanning, TaskId)>,
    on_task_creation: EventHandler<TaskCreation>,
) -> Element {
    let icon = rsx! { div { class: "h-5 w-5 flex-none", Todoist {} } };
    let mut project: Signal<Option<String>> = use_signal(|| None);
    let mut due_at = use_signal(|| Utc::now().format("%Y-%m-%d").to_string());
    let mut priority = use_signal(|| Some(TaskPriority::P4));
    let mut task_title = use_signal(|| "".to_string());
    let mut task_to_plan = use_signal(|| None);
    let mut force_validation = use_signal(|| false);
    let mut current_notification_id: Signal<Option<NotificationId>> = use_signal(|| None);
    let mut current_task_service_integration_connection_id: Signal<
        Option<IntegrationConnectionId>,
    > = use_signal(|| None);

    let mut mounted_element: Signal<Option<web_sys::Element>> = use_signal(|| None);

    use_drop(move || {
        if let Some(element) = mounted_element() {
            forget_flyonui_modal(&element);
        }
    });

    let _ = use_memo(move || {
        if current_notification_id() != Some(notification_to_plan().id) {
            *current_notification_id.write() = Some(notification_to_plan().id);
            if let Some(task) = notification_to_plan().task {
                task_title.write().clone_from(&task.title);
                *project.write() = Some(task.project.clone());
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
            }
        }

        if notification_to_plan().task.is_none()
            && let LoadState::Loaded(Some(IntegrationConnection {
                id,
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
            })) = task_service_integration_connection()
            && !create_notification_from_inbox_task
            && Some(id) != current_task_service_integration_connection_id()
        {
            *current_task_service_integration_connection_id.write() = Some(id);
            if project.peek().is_none() {
                *project.write() = Some(TODOIST_INBOX_PROJECT.to_string());
            }
        }
    });

    rsx! {
        div {
            id: "task-planning-modal",
            class: "overlay modal overlay-open:opacity-100 hidden overlay-open:duration-300",
            role: "dialog",
            tabindex: "-1",
            onmounted: move |element| {
                let web_element = element.as_web_event();
                init_flyonui_modal(&web_element);
                mounted_element.set(Some(web_element));
            },

            div {
                class: "modal-dialog overlay-open:opacity-100 overlay-open:duration-300",
                div {
                    class: "modal-content",
                    div {
                        class: "modal-header",
                        h3 { class: "modal-title", "Plan task" }
                        button {
                            r#type: "button",
                            class: "btn btn-text btn-circle btn-sm absolute end-3 top-3",
                            "aria-label": "Close",
                            "data-overlay": "#task-planning-modal",
                            span { class: "icon-[tabler--x] size-4" }
                        }
                    }

                    form {
                        class: "flex flex-col",
                        method: "dialog",
                        onsubmit: move |evt| {
                            evt.prevent_default();
                            if let Some(task) = task_to_plan() {
                                if let Some(task_planning_parameters) = validate_planning_form(
                                    &evt.data.values(), project()
                                ) {
                                    on_task_planning.call((task_planning_parameters, task.id));
                                    close_flyonui_modal("#task-planning-modal");
                                } else {
                                    *force_validation.write() = true;
                                }
                            } else if let Some(task_creation_parameters) = validate_creation_form(
                                &evt.data.values(), project()
                            ) {
                                on_task_creation.call(task_creation_parameters);
                                close_flyonui_modal("#task-planning-modal");
                            } else {
                                *force_validation.write() = true;
                            }
                        },

                        div {
                            class: "modal-body overflow-visible pt-2 space-y-4",

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

                            FloatingLabelInputSearchSelect::<ProjectSummary> {
                                name: "project-search-input".to_string(),
                                label: "Project",
                                required: true,
                                data_select: json!({
                                    "value": project(),
                                    "apiUrl": format!("{api_base_url}tasks/projects/search"),
                                    "apiSearchQueryKey": "matches",
                                    "apiFieldsMap": {
                                        "id": "source_id",
                                        "val": "name",
                                        "title": "name"
                                    }
                                }),
                                on_select: move |selected_project: Option<ProjectSummary>| {
                                    *project.write() = selected_project.map(|p| p.name);
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
                            }

                            FloatingLabelSelect::<TaskPriority> {
                                name: "task-priority-input".to_string(),
                                label: Some("Priority".to_string()),
                                required: false,
                                force_validation: force_validation(),
                                default_value: "{priority().unwrap_or_default()}",
                                on_select: move |selected_priority| {
                                    *priority.write() = selected_priority;
                                },

                                option { selected: priority() == Some(TaskPriority::P1), value: "1", "🔴 Priority 1" }
                                option { selected: priority() == Some(TaskPriority::P2), value: "2", "🟠 Priority 2" }
                                option { selected: priority() == Some(TaskPriority::P3), value: "3", "🟡 Priority 3" }
                                option { selected: priority() == Some(TaskPriority::P4), value: "4", "🔵 Priority 4" }
                            }
                        }

                        div {
                            class: "modal-footer",

                            button {
                                id: "task-planning-modal-submit",
                                tabindex: 0,
                                "type": "submit",
                                class: "btn btn-primary w-full",
                                //"data-overlay": "#task-planning-modal",
                                "Plan"
                            }
                        }
                    }
                }
            }
        }
    }
}

fn get_form_text<'a>(values: &'a [(String, FormValue)], name: &str) -> Option<&'a str> {
    values.iter().find_map(|(k, v)| {
        if k == name {
            match v {
                FormValue::Text(s) => Some(s.as_str()),
                _ => None,
            }
        } else {
            None
        }
    })
}

fn validate_planning_form(
    values: &[(String, FormValue)],
    selected_project: Option<String>,
) -> Option<TaskPlanning> {
    let due_at = get_form_text(values, "task-due_at-input").map_or(Ok(None), |value| {
        if value.is_empty() {
            Ok(None)
        } else {
            value.parse::<DueDate>().map(Some)
        }
    });
    let priority = get_form_text(values, "task-priority-input").map_or(
        Err("Task priority value is required".to_string()),
        |value| value.parse::<TaskPriority>(),
    );

    // Buggy because of https://github.com/themeselection/flyonui/issues/86
    // workaround:
    let project_name = selected_project.ok_or("Task project is required");

    if let (Ok(project_name), Ok(due_at), Ok(priority)) = (project_name, due_at, priority) {
        return Some(TaskPlanning {
            project_name,
            due_at,
            priority,
        });
    }

    None
}

fn validate_creation_form(
    values: &[(String, FormValue)],
    selected_project: Option<String>,
) -> Option<TaskCreation> {
    let title = get_form_text(values, "task-title-input")
        .ok_or_else(|| "Task title is required".to_string());

    let due_at = get_form_text(values, "task-due_at-input").map_or(Ok(None), |value| {
        if value.is_empty() {
            Ok(None)
        } else {
            value.parse::<DueDate>().map(Some)
        }
    });

    let priority = get_form_text(values, "task-priority-input").map_or(
        Err("Task priority value is required".to_string()),
        |value| value.parse::<TaskPriority>(),
    );

    // Buggy because of https://github.com/themeselection/flyonui/issues/86
    // workaround:
    let project_name = selected_project.ok_or("Task project is required");

    if let (Ok(title), Ok(project_name), Ok(due_at), Ok(priority)) =
        (title, project_name, due_at, priority)
    {
        return Some(TaskCreation {
            title: title.to_string(),
            body: None,
            project_name: Some(project_name),
            due_at,
            priority,
        });
    }

    None
}
