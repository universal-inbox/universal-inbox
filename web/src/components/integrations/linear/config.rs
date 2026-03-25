#![allow(non_snake_case)]
use dioxus::prelude::*;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::linear::{LinearConfig, LinearSyncTaskConfig},
        provider::IntegrationProviderKind,
    },
    task::{PresetDueDate, ProjectSummary},
};

use crate::{
    components::{
        flyonui::tooltip::{Tooltip, TooltipPlacement},
        project_search_field::ProjectSearchField,
        settings_controls::SettingRow,
        task_manager_picker::{ProviderIcon, resolve_task_manager_kind},
        ui::{
            TaskMgrOption, TaskMgrValue, ToggleSize, ToggleSwitch, UISelect, UISelectOption,
            preset_due_date_options,
        },
    },
    config::get_api_base_url,
    model::{LoadState, UniversalInboxUIModel},
    services::integration_connection_service::TASK_SERVICE_INTEGRATION_CONNECTIONS,
};

#[component]
pub fn LinearProviderConfiguration(
    config: ReadSignal<LinearConfig>,
    ui_model: Signal<UniversalInboxUIModel>,
    on_config_change: EventHandler<IntegrationConnectionConfig>,
) -> Element {
    let mut default_project: Signal<Option<ProjectSummary>> = use_signal(|| None);
    let mut default_due_at: Signal<Option<PresetDueDate>> = use_signal(|| None);
    let mut default_task_manager_provider_kind: Signal<Option<IntegrationProviderKind>> =
        use_signal(|| None);
    let mut task_config_enabled = use_signal(|| false);
    use_memo(move || {
        *default_project.write() = config().sync_task_config.target_project;
        default_due_at
            .write()
            .clone_from(&config().sync_task_config.default_due_at);
        *default_task_manager_provider_kind.write() =
            config().sync_task_config.task_manager_provider_kind;
        *task_config_enabled.write() = if !ui_model.read().is_task_actions_enabled {
            false
        } else {
            config().sync_task_config.enabled
        };
    });
    let show_task_manager_select = matches!(
        &*TASK_SERVICE_INTEGRATION_CONNECTIONS.read(),
        LoadState::Loaded(connections) if connections.len() >= 2
    );
    let collapse_style = use_memo(move || {
        if task_config_enabled() {
            ""
        } else {
            "hidden overflow-hidden"
        }
    });
    let api_base_url = get_api_base_url().unwrap();
    let as_tasks_disabled = !ui_model.read().is_task_actions_enabled;

    rsx! {
        SettingRow {
            label: rsx! { "Synchronize Linear notifications" },
            ToggleSwitch {
                size: ToggleSize::Md,
                checked: config().sync_notifications_enabled,
                onchange: move |new_value: bool| {
                    on_config_change.call(IntegrationConnectionConfig::Linear(LinearConfig {
                        sync_notifications_enabled: new_value,
                        ..config()
                    }))
                },
            }
        }

        Tooltip {
            placement: TooltipPlacement::Bottom,
            disabled: !as_tasks_disabled,
            tooltip_class: "tooltip-error",
            text: "A task management service must be connected to enable this feature",

            SettingRow {
                label: rsx! { "Synchronize Linear assigned issues as tasks" },
                ToggleSwitch {
                    size: ToggleSize::Md,
                    checked: config().sync_task_config.enabled,
                    disabled: as_tasks_disabled,
                    onchange: move |new_value: bool| {
                        on_config_change.call(IntegrationConnectionConfig::Linear(LinearConfig {
                            sync_task_config: LinearSyncTaskConfig {
                                enabled: new_value,
                                ..config().sync_task_config
                            },
                            ..config()
                        }))
                    },
                }
            }
        }

        div {
            class: "collapse transition-[height] duration-300 {collapse_style} pb-0 pr-0",

            SettingRow {
                label: rsx! { "Project to assign synchronized tasks to" },
                ProjectSearchField {
                    api_base_url: api_base_url.clone(),
                    selected_project: default_project,
                    provider_kind: Some(resolve_task_manager_kind(default_task_manager_provider_kind())),
                    on_change: move |project: Option<ProjectSummary>| {
                        on_config_change.call(IntegrationConnectionConfig::Linear(LinearConfig {
                            sync_task_config: LinearSyncTaskConfig {
                                target_project: project.clone(),
                                ..config().sync_task_config
                            },
                            ..config()
                        }))
                    },
                    name: "linear-project-search-input".to_string(),
                    disabled: !ui_model.read().is_task_actions_enabled,
                    width: "260px".to_string(),
                }
            }

            SettingRow {
                label: rsx! { "Due date to assign to synchronized tasks" },
                UISelect::<PresetDueDate> {
                    value: default_due_at,
                    options: preset_due_date_options(),
                    on_change: move |default_due_at| {
                        on_config_change.call(IntegrationConnectionConfig::Linear(LinearConfig {
                            sync_task_config: LinearSyncTaskConfig {
                                default_due_at,
                                ..config().sync_task_config
                            },
                            ..config()
                        }));
                    },
                    placeholder: "Pick a due date…".to_string(),
                    allow_clear: true,
                    disabled: !ui_model.read().is_task_actions_enabled,
                    width: "260px".to_string(),
                    name: "task-due-at-input".to_string(),
                }
            }

            if show_task_manager_select {
                SettingRow {
                    label: rsx! { "Task manager to sync with" },
                    UISelect::<IntegrationProviderKind> {
                        value: default_task_manager_provider_kind,
                        options: vec![
                            UISelectOption::new(IntegrationProviderKind::Todoist, "Todoist"),
                            UISelectOption::new(IntegrationProviderKind::TickTick, "TickTick"),
                        ],
                        on_change: move |task_manager_provider_kind| {
                            *default_project.write() = None;
                            on_config_change.call(IntegrationConnectionConfig::Linear(LinearConfig {
                                sync_task_config: LinearSyncTaskConfig {
                                    task_manager_provider_kind,
                                    target_project: None,
                                    ..config().sync_task_config
                                },
                                ..config()
                            }));
                        },
                        placeholder: "Pick a task manager…".to_string(),
                        disabled: !ui_model.read().is_task_actions_enabled,
                        width: "260px".to_string(),
                        name: "linear-task-manager-input".to_string(),
                        render_value: use_callback(move |opt: UISelectOption<IntegrationProviderKind>| {
                            rsx! { TaskMgrValue {
                                logo: rsx! { ProviderIcon { kind: Some(opt.value) } },
                                label: opt.label,
                            } }
                        }),
                        render_option: use_callback(move |opt: UISelectOption<IntegrationProviderKind>| {
                            rsx! { TaskMgrOption {
                                logo: rsx! { ProviderIcon { kind: Some(opt.value) } },
                                label: opt.label,
                            } }
                        }),
                    }
                }
            }
        }
    }
}
