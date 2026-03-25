#![allow(non_snake_case)]
use dioxus::prelude::*;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::ticktick::TickTickConfig,
        provider::IntegrationProviderKind,
    },
    task::{
        PresetDueDate, ProjectSummary, TaskPriority, integrations::ticktick::TICKTICK_INBOX_PROJECT,
    },
};

use crate::{
    components::{
        project_search_field::ProjectSearchField,
        settings_controls::SettingRow,
        ui::{
            ToggleSize, ToggleSwitch, UISelect, preset_due_date_options, priority_select_renderers,
            task_priority_options,
        },
    },
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
    let (priority_render_value, priority_render_option) = priority_select_renderers();

    rsx! {
        SettingRow {
            label: rsx! { "Synchronize TickTick tasks" },
            ToggleSwitch {
                size: ToggleSize::Md,
                checked: config().sync_tasks_enabled,
                onchange: move |new_value: bool| {
                    on_config_change.call(IntegrationConnectionConfig::TickTick(TickTickConfig {
                        sync_tasks_enabled: new_value,
                        ..config()
                    }))
                },
            }
        }


        SettingRow {
            label: rsx! {
                "Synchronize TickTick tasks from "
                code { "#{TICKTICK_INBOX_PROJECT}" }
                " as notifications"
            },
            ToggleSwitch {
                size: ToggleSize::Md,
                checked: config().create_notification_from_inbox_task,
                onchange: move |new_value: bool| {
                    on_config_change.call(IntegrationConnectionConfig::TickTick(TickTickConfig {
                        create_notification_from_inbox_task: new_value,
                        ..config()
                    }))
                },
            }
        }

        div {
            class: "settings-subsection",
            div {
                class: "settings-subsection-title",
                "Default task settings"
            }

            SettingRow {
                label: rsx! { "Project to assign new tasks" },
                ProjectSearchField {
                    api_base_url: api_base_url.clone(),
                    selected_project: default_project,
                    provider_kind: Some(IntegrationProviderKind::TickTick),
                    on_change: move |default_project: Option<ProjectSummary>| {
                        on_config_change.call(IntegrationConnectionConfig::TickTick(TickTickConfig {
                            default_project,
                            ..config()
                        }))
                    },
                    name: "star-project-search-input".to_string(),
                    width: "260px".to_string(),
                }
            }

            SettingRow {
                label: rsx! { "Due date to assign to new tasks" },
                UISelect::<PresetDueDate> {
                    value: default_due_at,
                    options: preset_due_date_options(),
                    on_change: move |default_due_at| {
                        on_config_change.call(IntegrationConnectionConfig::TickTick(TickTickConfig {
                            default_due_at,
                            ..config()
                        }));
                    },
                    placeholder: "Pick a due date…".to_string(),
                    allow_clear: true,
                    width: "260px".to_string(),
                    name: "task-due-at-input".to_string(),
                }
            }

            SettingRow {
                label: rsx! { "Priority to assign to new tasks" },
                UISelect::<TaskPriority> {
                    value: default_priority,
                    options: task_priority_options(),
                    on_change: move |default_priority: Option<TaskPriority>| {
                        on_config_change.call(IntegrationConnectionConfig::TickTick(TickTickConfig {
                            default_priority,
                            ..config()
                        }));
                    },
                    placeholder: "Pick a priority…".to_string(),
                    width: "260px".to_string(),
                    name: "task-priority-input".to_string(),
                    render_value: priority_render_value,
                    render_option: priority_render_option,
                }
            }
        }
    }
}
