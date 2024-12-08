#![allow(non_snake_case)]
use dioxus::prelude::*;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::linear::{LinearConfig, LinearSyncTaskConfig},
    },
    task::{PresetDueDate, ProjectSummary},
};

use crate::{
    components::{
        floating_label_inputs::FloatingLabelSelect,
        integrations::task_project_search::TaskProjectSearch,
    },
    model::UniversalInboxUIModel,
};

#[component]
pub fn LinearProviderConfiguration(
    config: ReadOnlySignal<LinearConfig>,
    ui_model: Signal<UniversalInboxUIModel>,
    on_config_change: EventHandler<IntegrationConnectionConfig>,
) -> Element {
    let mut default_project: Signal<Option<String>> = use_signal(|| None);
    let mut default_due_at: Signal<Option<PresetDueDate>> = use_signal(|| None);
    let selected_project: Signal<Option<ProjectSummary>> = use_signal(|| None);
    let mut task_config_enabled = use_signal(|| false);
    use_effect(move || {
        *default_project.write() = config()
            .sync_task_config
            .target_project
            .map(|p| p.name.clone());
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
            "collapse-open"
        } else {
            "collapse-close"
        }
    });

    rsx! {
        div {
            class: "flex flex-col",

            div {
                class: "form-control",
                label {
                    class: "cursor-pointer label py-1",
                    span {
                        class: "label-text",
                        "Synchronize Linear notifications"
                    }
                    input {
                        r#type: "checkbox",
                        class: "toggle toggle-ghost",
                        oninput: move |event| {
                            on_config_change.call(IntegrationConnectionConfig::Linear(LinearConfig {
                                sync_notifications_enabled: event.value() == "true",
                                ..config()
                            }))
                        },
                        checked: config().sync_notifications_enabled
                    }
                }
            }

            div {
                class: "collapse {collapse_style} overflow-visible",

                div {
                    class: "form-control collapse-title p-0 min-h-0",
                    label {
                        class: "cursor-pointer label py-1",
                        span {
                            class: "label-text",
                            "Synchronize Linear assigned issues as tasks"
                        }
                        if !ui_model.read().is_task_actions_enabled {
                            span {
                                class: "label-text text-error",
                                "A task management service must be connected to enable this feature"
                            }
                        }
                        input {
                            r#type: "checkbox",
                            class: "toggle toggle-ghost",
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
                    }
                }

                div {
                    class: "collapse-content pb-0 pr-0",

                    div {
                        class: "form-control",
                        label {
                            class: "cursor-pointer label py-1",
                            span {
                                class: "label-text",
                                "Project to assign synchronized tasks to"
                            }
                            TaskProjectSearch {
                                class: "w-full max-w-xs bg-base-100 rounded",
                                default_project_name: default_project().unwrap_or_default(),
                                selected_project: selected_project,
                                ui_model: ui_model,
                                filter_out_inbox: false,
                                disabled: !ui_model.read().is_task_actions_enabled,
                                on_select: move |project: ProjectSummary| {
                                    on_config_change.call(IntegrationConnectionConfig::Linear(LinearConfig {
                                        sync_task_config: LinearSyncTaskConfig {
                                            target_project: Some(project.clone()),
                                            ..config().sync_task_config
                                        },
                                        ..config()
                                    }))
                                }
                            }
                        }
                    }

                    div {
                        class: "form-control",
                        label {
                            class: "cursor-pointer label py-1",
                            span {
                                class: "label-text",
                                "Due date to assign to synchronized tasks"
                            }

                            FloatingLabelSelect::<PresetDueDate> {
                                label: None,
                                class: "w-full max-w-xs bg-base-100 rounded",
                                name: "task-due-at-input".to_string(),
                                disabled: !ui_model.read().is_task_actions_enabled,
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
}
