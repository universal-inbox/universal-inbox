#![allow(non_snake_case)]
use dioxus::prelude::*;
use fermi::UseAtomRef;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::slack::{SlackConfig, SlackSyncTaskConfig, SlackSyncType},
    },
    task::{PresetDueDate, ProjectSummary, TaskPriority},
};

use crate::{
    components::{
        floating_label_inputs::FloatingLabelSelect,
        integrations::task_project_search::TaskProjectSearch,
    },
    model::UniversalInboxUIModel,
};

#[component]
pub fn SlackProviderConfiguration<'a>(
    cx: Scope,
    config: SlackConfig,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
    on_config_change: EventHandler<'a, IntegrationConnectionConfig>,
) -> Element {
    let default_priority = use_state(cx, || Some(TaskPriority::P4));
    let default_due_at: &UseState<Option<PresetDueDate>> = use_state(cx, || None);
    let default_project: &UseState<Option<String>> = use_state(cx, || None);
    let task_config_enabled = use_state(cx, || false);
    let _ = use_memo(cx, config, |config| {
        if let SlackSyncType::AsTasks(config) = config.sync_type {
            default_priority.set(Some(config.default_priority));
            default_due_at.set(config.default_due_at.clone());
            default_project.set(config.target_project.map(|p| p.name.clone()));
            task_config_enabled.set(true);
        } else {
            task_config_enabled.set(false);
        }
    });
    let collapse_style = use_memo(cx, (task_config_enabled,), |(task_config_enabled,)| {
        if *task_config_enabled {
            "collapse-open"
        } else {
            "collapse-close"
        }
    });
    let selected_project: &UseState<Option<ProjectSummary>> = use_state(cx, || None);

    render! {
        div {
            class: "flex flex-col",

            div {
                class: "form-control",
                label {
                    class: "cursor-pointer label py-1",
                    span {
                        class: "label-text",
                        "Synchronize Slack \"saved for later\" items"
                    }
                    input {
                        r#type: "checkbox",
                        class: "toggle toggle-ghost",
                        oninput: move |event| {
                            on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                                sync_enabled: event.value == "true",
                                ..config.clone()
                            }))
                        },
                        checked: config.sync_enabled
                    }
                }
            }

            div {
                class: "form-control",
                label {
                    class: "cursor-pointer label py-1",
                    span {
                        class: "label-text",
                        "Synchronize Slack \"saved for later\" items as notifications"
                    }
                    input {
                        r#type: "radio",
                        disabled: !config.sync_enabled,
                        name: "sync-type",
                        class: "radio radio-ghost",
                        oninput: move |_event| {
                            on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                                sync_type: SlackSyncType::AsNotifications,
                                ..config.clone()
                            }))
                        },
                        checked: config.sync_type == SlackSyncType::AsNotifications
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
                            "Synchronize Slack \"saved for later\" items as tasks"
                        }
                        input {
                            r#type: "radio",
                            disabled: !config.sync_enabled,
                            name: "sync-type",
                            class: "radio radio-ghost",
                            oninput: move |_event| {
                                on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                                    sync_type: SlackSyncType::AsTasks(match &config.sync_type {
                                        SlackSyncType::AsTasks(config) => config.clone(),
                                        _ => Default::default(),
                                    }),
                                    ..config.clone()
                                }))
                            },
                            checked: !(config.sync_type == SlackSyncType::AsNotifications)
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
                                default_project_name: (*default_project.current()).clone().unwrap_or_default(),
                                selected_project: selected_project.clone(),
                                ui_model_ref: ui_model_ref.clone(),
                                filter_out_inbox: false,
                                on_select: move |project: ProjectSummary| {
                                    on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                                        sync_type: SlackSyncType::AsTasks(match &config.sync_type {
                                            SlackSyncType::AsTasks(config) => SlackSyncTaskConfig {
                                                target_project: Some(project.clone()),
                                                ..config.clone()
                                            },
                                            _ => Default::default(),
                                        }),
                                        ..config.clone()
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
                                value: default_due_at.clone(),
                                on_select: move |default_due_at| {
                                    on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                                        sync_type: SlackSyncType::AsTasks(match &config.sync_type {
                                            SlackSyncType::AsTasks(task_config) => SlackSyncTaskConfig {
                                                default_due_at,
                                                ..task_config.clone()
                                            },
                                            _ => SlackSyncTaskConfig {
                                                default_due_at,
                                                ..Default::default()
                                            }
                                        }),
                                        ..config.clone()
                                    }));
                                },

                                option { PresetDueDate::Today.to_string() }
                                option { PresetDueDate::Tomorrow.to_string() }
                                option { PresetDueDate::ThisWeekend.to_string() }
                                option { PresetDueDate::NextWeek.to_string() }
                            }
                        }
                    }

                    div {
                        class: "form-control",
                        label {
                            class: "cursor-pointer label py-1",
                            span {
                                class: "label-text",
                                "Priority to assign to synchronized tasks"
                            }

                            FloatingLabelSelect::<TaskPriority> {
                                label: None,
                                class: "w-full max-w-xs bg-base-100 rounded",
                                name: "task-priority-input".to_string(),
                                value: default_priority.clone(),
                                required: true,
                                on_select: move |priority: Option<TaskPriority>| {
                                    on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                                        sync_type: SlackSyncType::AsTasks(match &config.sync_type {
                                            SlackSyncType::AsTasks(task_config) => SlackSyncTaskConfig {
                                                default_priority: priority.unwrap_or_default(),
                                                ..task_config.clone()
                                            },
                                            _ => SlackSyncTaskConfig {
                                                default_priority: priority.unwrap_or_default(),
                                                ..Default::default()
                                            },
                                        }),
                                        ..config.clone()
                                    }));
                                },

                                option { value: "1", "🔴 Priority 1" }
                                option { value: "2", "🟠 Priority 2" }
                                option { value: "3", "🟡 Priority 3" }
                                option { value: "4", "🔵 Priority 4" }
                            }
                        }
                    }
                }
            }
        }
    }
}
