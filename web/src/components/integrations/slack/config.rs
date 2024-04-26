#![allow(non_snake_case)]
use dioxus::prelude::*;

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
pub fn SlackProviderConfiguration(
    config: ReadOnlySignal<SlackConfig>,
    ui_model: Signal<UniversalInboxUIModel>,
    on_config_change: EventHandler<IntegrationConnectionConfig>,
) -> Element {
    let mut default_priority = use_signal(|| Some(TaskPriority::P4));
    let mut default_due_at: Signal<Option<PresetDueDate>> = use_signal(|| None);
    let mut default_project: Signal<Option<String>> = use_signal(|| None);
    let selected_project: Signal<Option<ProjectSummary>> = use_signal(|| None);
    let mut task_config_enabled = use_signal(|| false);
    let _ = use_memo(move || {
        if let SlackSyncType::AsTasks(config) = config().sync_type {
            *default_priority.write() = Some(config.default_priority);
            default_due_at.write().clone_from(&config.default_due_at);
            *default_project.write() = config.target_project.map(|p| p.name.clone());
            *task_config_enabled.write() = true;
        } else {
            *task_config_enabled.write() = false;
        }
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
                        "Synchronize Slack \"saved for later\" items"
                    }
                    input {
                        r#type: "checkbox",
                        class: "toggle toggle-ghost",
                        oninput: move |event| {
                            on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                                sync_enabled: event.value() == "true",
                                ..config()
                            }))
                        },
                        checked: config().sync_enabled
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
                        disabled: !config().sync_enabled,
                        name: "sync-type",
                        class: "radio radio-ghost",
                        oninput: move |_event| {
                            on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                                sync_type: SlackSyncType::AsNotifications,
                                ..config()
                            }))
                        },
                        checked: config().sync_type == SlackSyncType::AsNotifications
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
                            disabled: !config().sync_enabled,
                            name: "sync-type",
                            class: "radio radio-ghost",
                            oninput: move |_event| {
                                on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                                    sync_type: SlackSyncType::AsTasks(match &config().sync_type {
                                        SlackSyncType::AsTasks(config) => config.clone(),
                                        _ => Default::default(),
                                    }),
                                    ..config()
                                }))
                            },
                            checked: !(config().sync_type == SlackSyncType::AsNotifications)
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
                                on_select: move |project: ProjectSummary| {
                                    on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                                        sync_type: SlackSyncType::AsTasks(match &config().sync_type {
                                            SlackSyncType::AsTasks(config) => SlackSyncTaskConfig {
                                                target_project: Some(project.clone()),
                                                ..config.clone()
                                            },
                                            _ => Default::default(),
                                        }),
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
                                on_select: move |default_due_at| {
                                    on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                                        sync_type: SlackSyncType::AsTasks(match &config().sync_type {
                                            SlackSyncType::AsTasks(task_config) => SlackSyncTaskConfig {
                                                default_due_at,
                                                ..task_config.clone()
                                            },
                                            _ => SlackSyncTaskConfig {
                                                default_due_at,
                                                ..Default::default()
                                            }
                                        }),
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
                                required: true,
                                on_select: move |priority: Option<TaskPriority>| {
                                    on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                                        sync_type: SlackSyncType::AsTasks(match &config().sync_type {
                                            SlackSyncType::AsTasks(task_config) => SlackSyncTaskConfig {
                                                default_priority: priority.unwrap_or_default(),
                                                ..task_config.clone()
                                            },
                                            _ => SlackSyncTaskConfig {
                                                default_priority: priority.unwrap_or_default(),
                                                ..Default::default()
                                            },
                                        }),
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
}
