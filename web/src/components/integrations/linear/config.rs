#![allow(non_snake_case)]
use dioxus::prelude::*;

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
pub fn LinearProviderConfiguration(
    config: ReadOnlySignal<LinearConfig>,
    ui_model: Signal<UniversalInboxUIModel>,
    on_config_change: EventHandler<IntegrationConnectionConfig>,
) -> Element {
    let mut default_project: Signal<Option<String>> = use_signal(|| None);
    let selected_project: Signal<Option<ProjectSummary>> = use_signal(|| None);
    let mut task_config_enabled = use_signal(|| false);
    let _ = use_memo(move || {
        *default_project.write() = config()
            .sync_task_config
            .target_project
            .map(|p| p.name.clone());
        *task_config_enabled.write() = config().sync_task_config.enabled;
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
                        input {
                            r#type: "checkbox",
                            class: "toggle toggle-ghost",
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
                }
            }
        }
    }
}
