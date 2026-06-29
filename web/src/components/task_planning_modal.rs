#![allow(non_snake_case)]

use chrono::Utc;
use dioxus::prelude::dioxus_core::use_drop;
use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionId,
        integrations::task_time_config::TaskTimeConfig,
        integrations::ticktick::TickTickConfig,
        integrations::todoist::TodoistConfig,
        provider::{IntegrationProvider, IntegrationProviderKind},
    },
    notification::{NotificationId, NotificationWithTask},
    task::{
        DueDate, ProjectId, ProjectSummary, TaskCreation, TaskId, TaskPlanning, TaskPriority,
        integrations::todoist::TODOIST_INBOX_PROJECT,
    },
    third_party::integrations::ticktick::TICKTICK_INBOX_PROJECT,
};
use url::Url;

use crate::{
    components::{
        datepicker::flatpickr,
        integrations::icons::NotificationIcon,
        project_search_field::ProjectSearchField,
        task_manager_picker::{default_task_manager_kind, user_default_task_manager_kind},
        task_time_config_row::TaskTimeConfigRow,
        ui::{
            ModalFooter, ModalHeader, ModalSourceRow, PriorityOption, PriorityValue,
            TaskAppTileSelect, UISelect, UISelectOption,
            button::{Button, ButtonSize, ButtonVariant},
            kbd::Kbd,
            task_priority_color, task_priority_options,
        },
    },
    model::{LoadState, UniversalInboxUIModel},
    services::flyonui::{close_flyonui_modal, forget_flyonui_modal, init_flyonui_modal},
};

#[component]
pub fn TaskPlanningModal(
    api_base_url: Url,
    notification_to_plan: ReadSignal<NotificationWithTask>,
    task_service_integration_connection: Signal<LoadState<Option<IntegrationConnection>>>,
    task_service_integration_connections: Signal<LoadState<Vec<IntegrationConnection>>>,
    ui_model: Signal<UniversalInboxUIModel>,
    on_task_planning: EventHandler<(TaskPlanning, TaskId)>,
    on_task_creation: EventHandler<TaskCreation>,
) -> Element {
    let mut selected_task_provider_kind: Signal<Option<IntegrationProviderKind>> =
        use_signal(|| None);
    let mut project: Signal<Option<String>> = use_signal(|| None);
    let mut due_at = use_signal(|| Utc::now().format("%Y-%m-%d").to_string());
    let mut priority = use_signal(|| Some(TaskPriority::P4));
    let mut time_config: Signal<Option<TaskTimeConfig>> = use_signal(|| None);
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
            && let LoadState::Loaded(connections) = task_service_integration_connections()
        {
            // Auto-select the default task service if none selected.
            // Prefer the user's configured preference, fall back to the first connection.
            if selected_task_provider_kind.peek().is_none()
                && let Some(kind) =
                    default_task_manager_kind(&connections, user_default_task_manager_kind())
            {
                *selected_task_provider_kind.write() = Some(kind);
            }

            // Set default project based on the selected provider
            if let Some(selected_kind) = selected_task_provider_kind()
                && let Some(connection) = connections
                    .iter()
                    .find(|c| c.provider.kind() == selected_kind)
            {
                let connection_id = connection.id;
                match &connection.provider {
                    IntegrationProvider::Todoist {
                        config:
                            TodoistConfig {
                                create_notification_from_inbox_task,
                                ..
                            },
                        ..
                    } if !create_notification_from_inbox_task
                        && Some(connection_id)
                            != current_task_service_integration_connection_id() =>
                    {
                        *current_task_service_integration_connection_id.write() =
                            Some(connection_id);
                        if project.peek().is_none() {
                            *project.write() = Some(TODOIST_INBOX_PROJECT.to_string());
                        }
                    }
                    IntegrationProvider::TickTick {
                        config:
                            TickTickConfig {
                                create_notification_from_inbox_task,
                                ..
                            },
                        ..
                    } if !create_notification_from_inbox_task
                        && Some(connection_id)
                            != current_task_service_integration_connection_id() =>
                    {
                        *current_task_service_integration_connection_id.write() =
                            Some(connection_id);
                        if project.peek().is_none() {
                            *project.write() = Some(TICKTICK_INBOX_PROJECT.to_string());
                        }
                    }
                    _ => {}
                }
            }
        }
    });

    let task_app_options = use_memo(move || match task_service_integration_connections() {
        LoadState::Loaded(connections) => connections
            .iter()
            .map(|c| {
                let kind = c.provider.kind();
                UISelectOption::new(kind, kind.to_string())
            })
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    });

    // Submit-CTA validity. In plan-existing-task mode the title is read-only —
    // only the project is required. In create mode both title and project are.
    let invalid = use_memo(move || {
        if task_to_plan.read().is_some() {
            project.read().is_none()
        } else {
            task_title.read().trim().is_empty() || project.read().is_none()
        }
    });

    let kind_for_selection = notification_to_plan().kind;
    let notification_title = notification_to_plan().title;

    rsx! {
        div {
            id: "task-planning-modal",
            class: "overlay modal overlay-open:opacity-100 hidden overlay-open:duration-300",
            role: "dialog",
            "aria-modal": "true",
            "aria-labelledby": "plan-task-title",
            tabindex: "-1",
            onmounted: move |element| {
                let web_element = element.as_web_event();
                init_flyonui_modal(&web_element);
                mounted_element.set(Some(web_element));
            },

            div {
                class: "modal-dialog overlay-open:opacity-100 overlay-open:duration-300",
                div {
                    class: "modal-content w-[460px] max-w-[calc(100vw-32px)] p-0 border border-ui-border shadow-ui-lg bg-ui-surface",

                    ModalHeader {
                        eyebrow: "From notification".to_string(),
                        title: "Plan a task".to_string(),
                        title_id: "plan-task-title".to_string(),
                        overlay_id: "#task-planning-modal".to_string(),
                    }

                    ModalSourceRow {
                        eyebrow: "Notification".to_string(),
                        title: notification_title.clone(),
                        tile: rsx! { NotificationIcon { kind: kind_for_selection } },
                    }

                    form {
                        method: "dialog",
                        onsubmit: move |evt| {
                            evt.prevent_default();
                            if invalid() {
                                *force_validation.write() = true;
                                return;
                            }
                            if let Some(task) = task_to_plan() {
                                if let Some(params) = build_planning(
                                    project(), &due_at.read(), priority(), time_config(),
                                ) {
                                    on_task_planning.call((params, task.id));
                                    close_flyonui_modal("#task-planning-modal");
                                } else {
                                    *force_validation.write() = true;
                                }
                            } else if let Some(params) = build_creation(
                                &task_title.read(), project(), &due_at.read(),
                                priority(), selected_task_provider_kind(), time_config(),
                            ) {
                                on_task_creation.call(params);
                                close_flyonui_modal("#task-planning-modal");
                            } else {
                                *force_validation.write() = true;
                            }
                        },

                        div { class: "flex flex-col gap-3 px-4 pt-3.5 pb-1",

                            div { class: "grid grid-cols-[44px_1fr] gap-2.5 items-end",
                                div { class: "flex flex-col gap-1.5",
                                    span { class: "text-[11px] font-semibold text-ui-base-muted tracking-[0.01em] inline-flex items-center gap-1", "App" }
                                    TaskAppTileSelect {
                                        value: selected_task_provider_kind,
                                        options: task_app_options(),
                                        on_change: move |kind: Option<IntegrationProviderKind>| {
                                            selected_task_provider_kind.set(kind);
                                            // Reset project when switching task manager.
                                            project.set(None);
                                        },
                                    }
                                }

                                div { class: "flex flex-col gap-1.5",
                                    label {
                                        r#for: "task-title-input",
                                        class: "text-[11px] font-semibold text-ui-base-muted tracking-[0.01em] inline-flex items-center gap-1",
                                        "Task title"
                                        span { class: "text-ui-error-hover ml-0.5", "aria-hidden": "true", "*" }
                                    }
                                    if task_to_plan().is_some() {
                                        div {
                                            id: "task-title-input",
                                            class: "flex items-center gap-2 h-[34px] px-2.5 border border-ui-border bg-ui-surface-alt rounded-ui-sm text-[13.5px] text-ui-base-content cursor-default",
                                            "{task_title}"
                                        }
                                    } else {
                                        div { class: "flex items-center gap-2 h-[34px] px-2.5 border border-ui-border bg-ui-surface-alt rounded-ui-sm text-[13.5px] text-ui-base-content focus-within:border-ui-primary focus-within:bg-ui-surface focus-within:shadow-[var(--ui-focus-ring)] transition-colors",
                                            input {
                                                id: "task-title-input",
                                                name: "task-title-input",
                                                r#type: "text",
                                                "aria-required": "true",
                                                autofocus: true,
                                                class: "flex-1 min-w-0 h-full bg-transparent border-none p-0 outline-none focus:outline-none focus-visible:outline-none placeholder:text-ui-base-muted",
                                                value: "{task_title}",
                                                oninput: move |evt| {
                                                    task_title.write().clone_from(&evt.value());
                                                },
                                            }
                                        }
                                    }
                                }
                            }

                            div { class: "flex flex-col gap-1.5",
                                span { class: "text-[11px] font-semibold text-ui-base-muted tracking-[0.01em] inline-flex items-center gap-1",
                                    "Project"
                                    span { class: "text-ui-error-hover ml-0.5", "aria-hidden": "true", "*" }
                                }
                                ModalProjectField {
                                    api_base_url: api_base_url.clone(),
                                    project,
                                    provider_kind: selected_task_provider_kind,
                                }
                            }

                            div { class: "grid grid-cols-2 gap-2.5",
                                div { class: "flex flex-col gap-1.5",
                                    label {
                                        r#for: "task-due_at-input",
                                        class: "text-[11px] font-semibold text-ui-base-muted tracking-[0.01em] inline-flex items-center gap-1",
                                        "Due date"
                                        span { class: "font-medium normal-case tracking-normal", "(optional)" }
                                    }
                                    div { class: "flex items-center gap-2 h-[34px] px-2.5 border border-ui-border bg-ui-surface-alt rounded-ui-sm text-[13.5px] text-ui-base-content focus-within:border-ui-primary focus-within:bg-ui-surface focus-within:shadow-[var(--ui-focus-ring)] transition-colors",
                                        span {
                                            class: "inline-flex items-center justify-center size-[18px] text-ui-base-muted shrink-0 icon-[lucide--calendar]",
                                            "aria-hidden": "true",
                                        }
                                        input {
                                            id: "task-due_at-input",
                                            name: "task-due_at-input",
                                            r#type: "text",
                                            placeholder: "Pick a date…",
                                            class: "flex-1 min-w-0 h-full bg-transparent border-none p-0 outline-none focus:outline-none focus-visible:outline-none placeholder:text-ui-base-muted",
                                            value: "{due_at}",
                                            oninput: move |evt| {
                                                due_at.write().clone_from(&evt.value());
                                            },
                                            onchange: move |evt| {
                                                due_at.write().clone_from(&evt.value());
                                            },
                                            onmounted: move |evt| {
                                                let element = evt.as_web_event();
                                                if let Ok(input) = element.dyn_into::<HtmlInputElement>() {
                                                    flatpickr(input);
                                                }
                                            },
                                        }
                                    }
                                }

                                div { class: "flex flex-col gap-1.5",
                                    span { class: "text-[11px] font-semibold text-ui-base-muted tracking-[0.01em] inline-flex items-center gap-1",
                                        "Priority"
                                        span { class: "font-medium normal-case tracking-normal", "(optional)" }
                                    }
                                    UISelect::<TaskPriority> {
                                        value: priority,
                                        options: task_priority_options(),
                                        on_change: move |p: Option<TaskPriority>| {
                                            *priority.write() = p;
                                        },
                                        placeholder: "Pick a priority…".to_string(),
                                        name: "task-priority-input".to_string(),
                                        width: "100%".to_string(),
                                        render_value: use_callback(move |opt: UISelectOption<TaskPriority>| {
                                            rsx! {
                                                PriorityValue {
                                                    color: task_priority_color(opt.value).to_string(),
                                                    label: opt.label,
                                                }
                                            }
                                        }),
                                        render_option: use_callback(move |opt: UISelectOption<TaskPriority>| {
                                            rsx! {
                                                PriorityOption {
                                                    color: task_priority_color(opt.value).to_string(),
                                                    label: opt.label,
                                                    meta: opt.meta,
                                                }
                                            }
                                        }),
                                    }
                                }
                            }

                            div { class: "flex items-center justify-between gap-2",
                                span { class: "text-[11px] font-semibold text-ui-base-muted tracking-[0.01em] inline-flex items-center gap-1",
                                    "Scheduled time"
                                    span { class: "font-medium normal-case tracking-normal", "(optional)" }
                                }
                                TaskTimeConfigRow {
                                    value: time_config,
                                    on_change: move |tc: Option<TaskTimeConfig>| time_config.set(tc),
                                }
                            }
                        }

                        ModalFooter {
                            hint: rsx! {
                                Kbd { label: "Tab".to_string() } span { "to move" }
                                span { class: "opacity-60", "·" }
                                Kbd { label: "Esc".to_string() } span { "cancel" }
                            },
                            Button {
                                variant: ButtonVariant::Ghost,
                                size: ButtonSize::Sm,
                                button_type: "button".to_string(),
                                data_overlay: "#task-planning-modal".to_string(),
                                onclick: move |_| close_flyonui_modal("#task-planning-modal"),
                                "Cancel"
                            }
                            Button {
                                variant: ButtonVariant::Primary,
                                size: ButtonSize::Sm,
                                button_type: "submit".to_string(),
                                disabled: invalid(),
                                "Plan task"
                                Kbd { label: "↵".to_string() }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Bridges the modal's legacy `Signal<Option<String>>` (project name only) with
/// the new `ProjectSearchField` which works in `Option<ProjectSummary>`.
/// External writes to `project` (e.g. inbox-project defaults) are mirrored
/// onto an internal summary signal so the search-select trigger stays in sync.
#[component]
fn ModalProjectField(
    api_base_url: ReadSignal<Url>,
    project: Signal<Option<String>>,
    provider_kind: ReadSignal<Option<IntegrationProviderKind>>,
) -> Element {
    let mut selected_project: Signal<Option<ProjectSummary>> = use_signal(|| None);

    use_effect(move || {
        let current_name = project();
        let selected_name = selected_project.peek().as_ref().map(|p| p.name.clone());
        if current_name != selected_name {
            selected_project.set(current_name.map(|name| ProjectSummary {
                source_id: ProjectId::from(name.clone()),
                name,
            }));
        }
    });

    rsx! {
        ProjectSearchField {
            api_base_url,
            selected_project,
            provider_kind,
            on_change: move |selected: Option<ProjectSummary>| {
                project.set(selected.map(|p| p.name));
            },
            name: "project-search-input".to_string(),
            placeholder: "Pick a project…".to_string(),
            width: "100%".to_string(),
        }
    }
}

/// Combine a parsed due date with the optional time config: when both a date
/// and a time config are present, upgrade the date to a timezone-aware
/// datetime (shared [`DueDate::with_time_config`]). A time config with no due
/// date has nothing to anchor to and is left to flow as task metadata only.
fn apply_time_config(
    due_at: Option<DueDate>,
    time_config: &Option<TaskTimeConfig>,
) -> Option<DueDate> {
    match (due_at, time_config) {
        (Some(due), Some(tc)) => Some(due.with_time_config(tc)),
        (due, _) => due,
    }
}

fn build_planning(
    selected_project: Option<String>,
    due_at_str: &str,
    priority: Option<TaskPriority>,
    time_config: Option<TaskTimeConfig>,
) -> Option<TaskPlanning> {
    let due_at = if due_at_str.is_empty() {
        Ok(None)
    } else {
        due_at_str.parse::<DueDate>().map(Some)
    };
    let priority = priority.ok_or("Task priority is required");
    let project_name = selected_project.ok_or("Task project is required");

    if let (Ok(project_name), Ok(due_at), Ok(priority)) = (project_name, due_at, priority) {
        return Some(TaskPlanning {
            project_name,
            due_at: apply_time_config(due_at, &time_config),
            priority,
            time_config,
        });
    }

    None
}

fn build_creation(
    title: &str,
    selected_project: Option<String>,
    due_at_str: &str,
    priority: Option<TaskPriority>,
    task_provider_kind: Option<IntegrationProviderKind>,
    time_config: Option<TaskTimeConfig>,
) -> Option<TaskCreation> {
    let title = title.trim();
    if title.is_empty() {
        return None;
    }
    let due_at = if due_at_str.is_empty() {
        Ok(None)
    } else {
        due_at_str.parse::<DueDate>().map(Some)
    };
    let priority = priority.ok_or("Task priority is required");
    let project_name = selected_project.ok_or("Task project is required");

    if let (Ok(project_name), Ok(due_at), Ok(priority)) = (project_name, due_at, priority) {
        return Some(TaskCreation {
            title: title.to_string(),
            body: None,
            project_name: Some(project_name),
            due_at: apply_time_config(due_at, &time_config),
            priority,
            task_provider_kind,
            time_config,
        });
    }

    None
}
