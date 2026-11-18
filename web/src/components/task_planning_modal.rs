#![allow(non_snake_case)]

use chrono::Utc;
use dioxus::prelude::dioxus_core::use_drop;
use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;

use universal_inbox::{
    context::ContextSummary,
    integration_connection::{
        IntegrationConnection, IntegrationConnectionId,
        integrations::task_time_config::TaskTimeConfig,
        integrations::ticktick::TickTickConfig,
        integrations::todoist::TodoistConfig,
        provider::{IntegrationProvider, IntegrationProviderKind},
    },
    notification::{NotificationId, NotificationWithTask},
    task::{
        DueDate, ProjectReference, ProjectSummary, TASK_TITLE_MAX_LENGTH, Task, TaskCreation,
        TaskId, TaskPlanning, TaskPriority, TaskSourceKind,
    },
    third_party::item::{ThirdPartyItemData, ThirdPartyItemSource, ThirdPartyItemSourceKind},
};
use url::Url;

use crate::{
    components::{
        datepicker::flatpickr,
        integrations::icons::NotificationIcon,
        project_search_field::{ProjectChoice, ProjectSearchField, reference_destination},
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
    // The picked project, by id; `initial_project` is the task's own when planning a task, to
    // move it only when the user picked another project
    let mut project: Signal<Option<ProjectReference>> = use_signal(|| None);
    let mut initial_project: Signal<Option<ProjectReference>> = use_signal(|| None);
    let mut bound_context: Signal<Option<ContextSummary>> = use_signal(|| None);
    let mut select_inbox_when_empty = use_signal(|| false);
    let mut default_project_error: Signal<Option<String>> = use_signal(|| None);
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
                // The task's own task manager, so a TickTick task gets the TickTick picker
                *selected_task_provider_kind.write() = task_manager_kind(&task);
                let current = task_project_reference(&task);
                *project.write() = current.clone();
                *initial_project.write() = current;
                *bound_context.write() = task.context.clone();
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
                && Some(connection.id) != current_task_service_integration_connection_id()
            {
                *current_task_service_integration_connection_id.write() = Some(connection.id);
                // The integration's default project, else its inbox (by flag, never by name).
                // The inbox is skipped when its tasks become notifications: the new task would
                // come back as a notification.
                let (default_project, inbox_allowed) = match &connection.provider {
                    IntegrationProvider::Todoist {
                        config:
                            TodoistConfig {
                                default_project,
                                create_notification_from_inbox_task,
                                ..
                            },
                        ..
                    }
                    | IntegrationProvider::TickTick {
                        config:
                            TickTickConfig {
                                default_project,
                                create_notification_from_inbox_task,
                                ..
                            },
                        ..
                    } => (
                        default_project.clone(),
                        !create_notification_from_inbox_task,
                    ),
                    _ => (None, false),
                };
                // A stale default opens the field empty with its error, never on the inbox
                let issue = connection.default_project_issue.clone();
                *default_project_error.write() = issue.as_ref().map(|issue| issue.to_string());
                // The inbox only stands in for no default at all: a default not usable here (a
                // legacy Todoist value) leaves the field empty, for the user to pick
                let has_default = default_project.is_some();
                if project.peek().is_none() {
                    *project.write() = usable_default_project(default_project, selected_kind)
                        .filter(|_| issue.is_none());
                }
                *select_inbox_when_empty.write() = inbox_allowed && issue.is_none() && !has_default;
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
    // A TickTick task without a project goes to the TickTick inbox, which the
    // TickTick picker cannot list.
    let invalid = use_memo(move || {
        let missing_project = project.read().is_none()
            && selected_task_provider_kind() != Some(IntegrationProviderKind::TickTick);
        if task_to_plan.read().is_some() {
            missing_project
        } else {
            task_title.read().trim().is_empty() || missing_project
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
                                    project(), initial_project(), selected_task_provider_kind(),
                                    &due_at.read(), priority(), time_config(),
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
                                                maxlength: TASK_TITLE_MAX_LENGTH as i64,
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
                                ProjectSearchField {
                                    api_base_url: api_base_url.clone(),
                                    value: project(),
                                    bound_context: bound_context(),
                                    provider_kind: selected_task_provider_kind(),
                                    on_change: move |choice: Option<ProjectChoice>| {
                                        project.set(choice.map(|choice| choice.reference));
                                    },
                                    error: default_project_error(),
                                    select_inbox_when_empty: select_inbox_when_empty(),
                                    allow_clear: false,
                                    placeholder: if selected_task_provider_kind() == Some(IntegrationProviderKind::TickTick) {
                                        "Inbox".to_string()
                                    } else {
                                        "Pick a project…".to_string()
                                    },
                                    name: "project-search-input".to_string(),
                                    width: "100%".to_string(),
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

/// The task manager a task lives in: its own for a Todoist or TickTick task, its sink's for a
/// task synced from another source
fn task_manager_kind(task: &Task) -> Option<IntegrationProviderKind> {
    match task.kind {
        TaskSourceKind::Todoist => Some(IntegrationProviderKind::Todoist),
        TaskSourceKind::TickTick => Some(IntegrationProviderKind::TickTick),
        _ => match task
            .sink_item
            .as_ref()
            .map(|item| item.get_third_party_item_source_kind())
        {
            Some(ThirdPartyItemSourceKind::Todoist) => Some(IntegrationProviderKind::Todoist),
            Some(ThirdPartyItemSourceKind::TickTick) => Some(IntegrationProviderKind::TickTick),
            _ => None,
        },
    }
}

/// The project a task is in, by id: its context, else its TickTick project (TickTick projects
/// are not mirrored as contexts yet, the name is the one TickTick gave at the last sync)
fn task_project_reference(task: &Task) -> Option<ProjectReference> {
    if let Some(context_id) = task.context_id {
        return Some(ProjectReference::Context { context_id });
    }
    let ticktick_item = [Some(&task.source_item), task.sink_item.as_ref()]
        .into_iter()
        .flatten()
        .find_map(|item| match &item.data {
            ThirdPartyItemData::TickTickItem(ticktick_item) => Some(ticktick_item),
            _ => None,
        })?;
    Some(ProjectReference::Project(ProjectSummary {
        source_id: ticktick_item.project_id.clone().into(),
        name: task.project.clone(),
    }))
}

fn parse_due_at(due_at_str: &str) -> Option<Option<DueDate>> {
    if due_at_str.is_empty() {
        Some(None)
    } else {
        due_at_str.parse::<DueDate>().ok().map(Some)
    }
}

/// The project the modal opens on: the integration's default, when it can be picked as is (a
/// context for Todoist, a TickTick project for TickTick). A legacy Todoist default, not
/// migrated to its context yet, opens the field empty: it is never sent as a TickTick project.
fn usable_default_project(
    default_project: Option<ProjectReference>,
    task_manager: IntegrationProviderKind,
) -> Option<ProjectReference> {
    default_project.filter(|reference| reference_destination(reference, task_manager).is_some())
}

/// `selected_project` is only sent when it differs from `initial_project`: planning a task
/// without touching its project never moves it
fn build_planning(
    selected_project: Option<ProjectReference>,
    initial_project: Option<ProjectReference>,
    task_manager: Option<IntegrationProviderKind>,
    due_at_str: &str,
    priority: Option<TaskPriority>,
    time_config: Option<TaskTimeConfig>,
) -> Option<TaskPlanning> {
    let due_at = parse_due_at(due_at_str)?;
    let priority = priority?;
    let task_manager = task_manager.unwrap_or(IntegrationProviderKind::Todoist);
    let destination = selected_project
        .filter(|project| Some(project) != initial_project.as_ref())
        .and_then(|project| reference_destination(&project, task_manager));

    Some(TaskPlanning {
        destination,
        due_at: apply_time_config(due_at, &time_config),
        priority,
        time_config,
    })
}

fn build_creation(
    title: &str,
    selected_project: Option<ProjectReference>,
    due_at_str: &str,
    priority: Option<TaskPriority>,
    task_provider_kind: Option<IntegrationProviderKind>,
    time_config: Option<TaskTimeConfig>,
) -> Option<TaskCreation> {
    let title = title.trim();
    if title.is_empty() {
        return None;
    }
    let due_at = parse_due_at(due_at_str)?;
    let priority = priority?;

    Some(TaskCreation {
        title: title.to_string(),
        body: None,
        // `None`: the task manager's inbox (TickTick only, the Todoist inbox is picked by id)
        destination: selected_project.and_then(|project| {
            reference_destination(
                &project,
                task_provider_kind.unwrap_or(IntegrationProviderKind::Todoist),
            )
        }),
        due_at: apply_time_config(due_at, &time_config),
        priority,
        task_provider_kind,
        time_config,
        parent_id: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use universal_inbox::{context::ContextId, task::TaskDestination};
    use uuid::Uuid;
    use wasm_bindgen_test::*;

    fn legacy_todoist_default() -> ProjectReference {
        ProjectReference::Project(ProjectSummary {
            source_id: "2222".into(),
            name: "Work".to_string(),
        })
    }

    /// Review finding 1: a Todoist default stored before contexts (`{source_id, name}`) is never
    /// sent as a TickTick destination; the modal opens empty for the user to pick
    #[wasm_bindgen_test]
    fn test_a_legacy_todoist_default_is_never_a_ticktick_destination() {
        assert_eq!(
            usable_default_project(
                Some(legacy_todoist_default()),
                IntegrationProviderKind::Todoist
            ),
            None
        );
        let creation = build_creation(
            "Plan",
            Some(legacy_todoist_default()),
            "",
            Some(TaskPriority::P4),
            Some(IntegrationProviderKind::Todoist),
            None,
        )
        .unwrap();
        assert_eq!(creation.destination, None);
    }

    #[wasm_bindgen_test]
    fn test_usable_defaults_keep_their_destination() {
        let context = ProjectReference::Context {
            context_id: ContextId(Uuid::nil()),
        };
        assert_eq!(
            usable_default_project(Some(context.clone()), IntegrationProviderKind::Todoist),
            Some(context.clone())
        );
        // The same `{source_id, name}` is a TickTick project for TickTick
        assert_eq!(
            build_creation(
                "Plan",
                Some(legacy_todoist_default()),
                "",
                Some(TaskPriority::P4),
                Some(IntegrationProviderKind::TickTick),
                None,
            )
            .unwrap()
            .destination,
            Some(TaskDestination::TickTickProject("2222".into()))
        );
        // Planning without touching the project never moves the task
        assert_eq!(
            build_planning(
                Some(context.clone()),
                Some(context),
                Some(IntegrationProviderKind::Todoist),
                "",
                Some(TaskPriority::P4),
                None,
            )
            .unwrap()
            .destination,
            None
        );
    }
}
