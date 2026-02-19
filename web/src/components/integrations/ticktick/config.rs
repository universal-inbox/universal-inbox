#![allow(non_snake_case)]
use dioxus::prelude::*;
use serde_json::json;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::ticktick::TickTickConfig,
    },
    task::{
        PresetDueDate, ProjectSummary, TaskPriority, integrations::ticktick::TICKTICK_INBOX_PROJECT,
    },
};

use crate::{
    components::floating_label_inputs::{FloatingLabelInputSearchSelect, FloatingLabelSelect},
    config::get_api_base_url,
};

#[component]
pub fn TickTickProviderConfiguration(
    config: ReadSignal<TickTickConfig>,
    on_config_change: EventHandler<IntegrationConnectionConfig>,
) -> Element {
    let mut default_priority = use_signal(|| Some(TaskPriority::P4));
    let mut default_due_at: Signal<Option<PresetDueDate>> = use_signal(|| None);
    let mut default_project: Signal<Option<ProjectSummary>> = use_signal(|| None);

    use_effect(move || {
        *default_priority.write() = config().default_priority;
        default_due_at.write().clone_from(&config().default_due_at);
        *default_project.write() = config().default_project;
    });

    let api_base_url = get_api_base_url().unwrap();

    rsx! {
        div {
            class: "flex flex-col gap-2",

            div {
                class: "flex items-center",
                label {
                    class: "label-text cursor-pointer grow text-sm text-base-content",
                    "Synchronize TickTick tasks"
                }
                div {
                    class: "relative inline-block",
                    input {
                        r#type: "checkbox",
                        class: "switch switch-primary switch-outline peer",
                        oninput: move |event| {
                            on_config_change.call(IntegrationConnectionConfig::TickTick(TickTickConfig {
                                sync_tasks_enabled: event.value() == "true",
                                ..config()
                            }))
                        },
                        checked: config().sync_tasks_enabled
                    }
                    span {
                        class: "icon-[tabler--check] text-primary absolute start-1 top-1 hidden size-4 peer-checked:block"
                    }
                    span {
                        class: "icon-[tabler--x] text-neutral absolute end-1 top-1 block size-4 peer-checked:hidden"
                    }
                }
            }

            div {
                class: "flex items-center gap-2",
                label {
                    class: "label-text cursor-pointer grow text-sm text-base-content",
                    "Synchronize TickTick tasks from "
                    code { "#{TICKTICK_INBOX_PROJECT}" }
                    " as notifications"
                }
                div {
                    class: "relative inline-block",
                    input {
                        r#type: "checkbox",
                        class: "switch switch-primary switch-outline peer",
                        oninput: move |event| {
                            on_config_change.call(IntegrationConnectionConfig::TickTick(TickTickConfig {
                                create_notification_from_inbox_task: event.value() == "true",
                                ..config()
                            }))
                        },
                        checked: config().create_notification_from_inbox_task
                    }
                    span {
                        class: "icon-[tabler--check] text-primary absolute start-1 top-1 hidden size-4 peer-checked:block"
                    }
                    span {
                        class: "icon-[tabler--x] text-neutral absolute end-1 top-1 block size-4 peer-checked:hidden"
                    }
                }
            }

            div {
                class: "card card-xs bg-base-200",
                div {
                    class: "card-header",
                    div {
                        class: "card-title",
                        "Default task settings"
                    }
                }

                div {
                    class: "card-body text-sm",
                    div {
                        class: "flex items-center gap-2",
                        label {
                            class: "label-text cursor-pointer grow text-sm text-base-content",
                            "Project to assign new tasks"
                        }
                        FloatingLabelInputSearchSelect ::<ProjectSummary> {
                            name: "star-project-search-input".to_string(),
                            class: "w-full max-w-xs bg-base-100 rounded-sm",
                            required: true,
                            data_select: json!({
                                "value": default_project().map(|p| p.source_id.to_string()),
                                "apiUrl": format!("{api_base_url}tasks/projects/search"),
                                "apiSearchQueryKey": "matches",
                                "apiFieldsMap": {
                                    "id": "source_id",
                                    "val": "source_id",
                                    "title": "name"
                                }
                            }),
                            on_select: move |default_project: Option<ProjectSummary>| {
                                on_config_change.call(IntegrationConnectionConfig::TickTick(TickTickConfig {
                                    default_project,
                                    ..config()
                                }))
                            }
                        }
                    }

                    div {
                        class: "flex items-center gap-2",
                        label {
                            class: "label-text cursor-pointer grow text-sm text-base-content",
                            "Due date to assign to new tasks"
                        }
                        FloatingLabelSelect ::<PresetDueDate> {
                            label: None,
                            class: "max-w-xs",
                            name: "task-due-at-input".to_string(),
                            default_value: default_due_at().map(|due| due.to_string()).unwrap_or_default(),
                            on_select: move |default_due_at| {
                                on_config_change.call(IntegrationConnectionConfig::TickTick(TickTickConfig {
                                    default_due_at,
                                    ..config()
                                }));
                            },

                            option { selected: default_due_at() == Some(PresetDueDate::Today), "{PresetDueDate::Today}" }
                            option { selected: default_due_at() == Some(PresetDueDate::Tomorrow), "{PresetDueDate::Tomorrow}" }
                            option { selected: default_due_at() == Some(PresetDueDate::ThisWeekend), "{PresetDueDate::ThisWeekend}" }
                            option { selected: default_due_at() == Some(PresetDueDate::NextWeek), "{PresetDueDate::NextWeek}" }
                        }
                    }

                    div {
                        class: "flex items-center gap-2",
                        label {
                            class: "label-text cursor-pointer grow text-sm text-base-content",
                            "Priority to assign to new tasks"
                        }
                        FloatingLabelSelect::<TaskPriority> {
                            label: None,
                            class: "max-w-xs",
                            name: "task-priority-input".to_string(),
                            required: true,
                            default_value: "{default_priority().unwrap_or_default()}",
                            on_select: move |default_priority: Option<TaskPriority>| {
                                on_config_change.call(IntegrationConnectionConfig::TickTick(TickTickConfig {
                                    default_priority,
                                    ..config()
                                }));
                            },

                            option { selected: default_priority() == Some(TaskPriority::P1), value: "1", "🔴 Priority 1" }
                            option { selected: default_priority() == Some(TaskPriority::P2), value: "2", "🟠 Priority 2" }
                            option { selected: default_priority() == Some(TaskPriority::P3), value: "3", "🟡 Priority 3" }
                            option { selected: default_priority() == Some(TaskPriority::P4), value: "4", "🔵 Priority 4" }
                        }
                    }
                }
            }
        }
    }
}
