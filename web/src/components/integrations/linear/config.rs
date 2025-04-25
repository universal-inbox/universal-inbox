#![allow(non_snake_case)]
use dioxus::prelude::*;
use serde_json::json;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::linear::{LinearConfig, LinearSyncTaskConfig},
    },
    task::{PresetDueDate, ProjectSummary},
};

use crate::{
    components::floating_label_inputs::{FloatingLabelInputSearchSelect, FloatingLabelSelect},
    config::get_api_base_url,
    model::UniversalInboxUIModel,
};

#[component]
pub fn LinearProviderConfiguration(
    config: ReadOnlySignal<LinearConfig>,
    ui_model: Signal<UniversalInboxUIModel>,
    on_config_change: EventHandler<IntegrationConnectionConfig>,
) -> Element {
    let mut default_project: Signal<Option<ProjectSummary>> = use_signal(|| None);
    let mut default_due_at: Signal<Option<PresetDueDate>> = use_signal(|| None);
    let mut task_config_enabled = use_signal(|| false);
    use_memo(move || {
        *default_project.write() = config().sync_task_config.target_project;
        default_due_at
            .write()
            .clone_from(&config().sync_task_config.default_due_at);
        *task_config_enabled.write() = if !ui_model.read().is_task_actions_enabled {
            false
        } else {
            config().sync_task_config.enabled
        };
    });
    let collapse_style = use_memo(move || {
        if task_config_enabled() {
            ""
        } else {
            "hidden overflow-hidden"
        }
    });
    let api_base_url = get_api_base_url().unwrap();

    rsx! {
        div {
            class: "flex flex-col gap-2",

            div {
                class: "flex items-center",
                label {
                    class: "label-text cursor-pointer grow text-sm text-base-content",
                    "Synchronize Linear notifications"
                }
                div {
                    class: "relative inline-block",
                    input {
                        r#type: "checkbox",
                        class: "switch switch-soft switch-outline switch-sm peer",
                        oninput: move |event| {
                            on_config_change.call(IntegrationConnectionConfig::Linear(LinearConfig {
                                sync_notifications_enabled: event.value() == "true",
                                ..config()
                            }))
                        },
                        checked: config().sync_notifications_enabled
                    }
                    span {
                        class: "icon-[tabler--check] text-primary-content absolute start-1 top-1 hidden size-4 peer-checked:block"
                    }
                    span {
                        class: "icon-[tabler--x] text-neutral-content absolute end-1 top-1 block size-4 peer-checked:hidden"
                    }
                }
            }

            div {
                class: "flex flex-col gap-2 overflow-visible",

                div {
                    class: "flex items-center",
                    label {
                        class: "label-text cursor-pointer grow text-sm text-base-content",
                        "for": "linear-issues-as-tasks",
                        "Synchronize Linear assigned issues as tasks"
                    }
                    if !ui_model.read().is_task_actions_enabled {
                        span {
                            class: "label-text text-error",
                            "A task management service must be connected to enable this feature"
                        }
                    }
                    div {
                        class: "relative inline-block",
                        input {
                            r#type: "checkbox",
                            class: "switch switch-soft switch-outline switch-sm peer",
                            disabled: !ui_model.read().is_task_actions_enabled,
                            oninput: move |event| {
                                on_config_change.call(IntegrationConnectionConfig::Linear(LinearConfig {
                                    sync_task_config: LinearSyncTaskConfig {
                                        enabled: event.value() == "true",
                                        ..config().sync_task_config
                                    },
                                    ..config()
                                }))
                            },
                            checked: config().sync_task_config.enabled
                        }
                        span {
                            class: "icon-[tabler--check] text-primary-content absolute start-1 top-1 hidden size-4 peer-checked:block"
                        }
                        span {
                            class: "icon-[tabler--x] text-neutral-content absolute end-1 top-1 block size-4 peer-checked:hidden"
                        }
                    }
                }

                div {
                    class: "collapse transition-[height] duration-300 {collapse_style} pb-0 pr-0 flex flex-col gap-2",

                    div {
                        class: "flex items-center",
                        label {
                            class: "label-text cursor-pointer grow text-sm text-base-content",
                            "Project to assign synchronized tasks to"
                        }
                        FloatingLabelInputSearchSelect::<ProjectSummary> {
                            name: "linear-project-search-input".to_string(),
                            class: "w-full max-w-xs bg-base-100 rounded-sm",
                            required: true,
                            disabled: !ui_model.read().is_task_actions_enabled,
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
                            on_select: move |project: Option<ProjectSummary>| {
                                on_config_change.call(IntegrationConnectionConfig::Linear(LinearConfig {
                                    sync_task_config: LinearSyncTaskConfig {
                                        target_project: project.clone(),
                                        ..config().sync_task_config
                                    },
                                    ..config()
                                }))
                            }
                        }
                    }

                    div {
                        class: "flex items-center",
                        label {
                            class: "label-text cursor-pointer grow text-sm text-base-content",
                            "Due date to assign to synchronized tasks"
                        }

                        FloatingLabelSelect::<PresetDueDate> {
                            label: None,
                            class: "max-w-xs",
                            name: "task-due-at-input".to_string(),
                            disabled: !ui_model.read().is_task_actions_enabled,
                            default_value: default_due_at().map(|due| due.to_string()).unwrap_or_default(),
                            on_select: move |default_due_at| {
                                on_config_change.call(IntegrationConnectionConfig::Linear(LinearConfig {
                                    sync_task_config: LinearSyncTaskConfig {
                                        default_due_at,
                                        ..config().sync_task_config
                                    },
                                    ..config()
                                }));
                            },

                            option { selected: default_due_at() == Some(PresetDueDate::Today), "{PresetDueDate::Today}" }
                            option { selected: default_due_at() == Some(PresetDueDate::Tomorrow), "{PresetDueDate::Tomorrow}" }
                            option { selected: default_due_at() == Some(PresetDueDate::ThisWeekend), "{PresetDueDate::ThisWeekend}" }
                            option { selected: default_due_at() == Some(PresetDueDate::NextWeek), "{PresetDueDate::NextWeek}" }
                        }
                    }
                }
            }
        }
    }
}
