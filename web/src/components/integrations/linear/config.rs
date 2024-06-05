#![allow(non_snake_case)]
use dioxus::prelude::*;

use fermi::UseAtomRef;
use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::linear::{LinearConfig, LinearSyncTaskConfig},
    },
    task::ProjectSummary,
};

use crate::{
    components::integrations::task_project_search::TaskProjectSearch, model::UniversalInboxUIModel,
};

#[component]
pub fn LinearProviderConfiguration<'a>(
    cx: Scope,
    config: LinearConfig,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
    on_config_change: EventHandler<'a, IntegrationConnectionConfig>,
) -> Element {
    let default_project: &UseState<Option<String>> = use_state(cx, || None);
    let selected_project: &UseState<Option<ProjectSummary>> = use_state(cx, || None);
    let task_config_enabled = use_state(cx, || false);
    let _ = use_memo(cx, config, |config| {
        default_project.set(
            config
                .sync_task_config
                .target_project
                .map(|p| p.name.clone()),
        );
        task_config_enabled.set(config.sync_task_config.enabled);
    });
    let collapse_style = use_memo(cx, (task_config_enabled,), |(task_config_enabled,)| {
        if *task_config_enabled {
            "collapse-open"
        } else {
            "collapse-close"
        }
    });

    render! {
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
                                sync_notifications_enabled: event.value == "true",
                                ..config.clone()
                            }))
                        },
                        checked: config.sync_notifications_enabled
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
                        input {
                            r#type: "checkbox",
                            class: "toggle toggle-ghost",
                            oninput: move |event| {
                                on_config_change.call(IntegrationConnectionConfig::Linear(LinearConfig {
                                    sync_task_config: LinearSyncTaskConfig {
                                        enabled: event.value == "true",
                                        ..config.sync_task_config.clone()
                                    },
                                    ..config.clone()
                                }))
                            },
                            checked: config.sync_task_config.enabled
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
                                    on_config_change.call(IntegrationConnectionConfig::Linear(LinearConfig {
                                        sync_task_config: LinearSyncTaskConfig {
                                            target_project: Some(project.clone()),
                                            ..config.sync_task_config.clone()
                                        },
                                        ..config.clone()
                                    }))
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
