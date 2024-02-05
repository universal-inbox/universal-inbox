#![allow(non_snake_case)]
use dioxus::prelude::*;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::todoist::TodoistConfig,
    },
    task::integrations::todoist::TODOIST_INBOX_PROJECT,
};

#[component]
pub fn TodoistProviderConfiguration<'a>(
    cx: Scope,
    config: TodoistConfig,
    on_config_change: EventHandler<'a, IntegrationConnectionConfig>,
) -> Element {
    render! {
        div {
            class: "flex flex-col",

            div {
                class: "form-control",
                label {
                    class: "cursor-pointer label",
                    span {
                        class: "label-text",
                        "Synchronize Todoist tasks"
                    }
                    input {
                        r#type: "checkbox",
                        class: "toggle toggle-ghost",
                        oninput: move |event| {
                            on_config_change.call(IntegrationConnectionConfig::Todoist(TodoistConfig {
                                sync_tasks_enabled: event.value == "true",
                                ..config.clone()
                            }))
                        },
                        checked: config.sync_tasks_enabled
                    }
                }
            }

            div {
                class: "form-control",
                label {
                    class: "cursor-pointer label",
                    div {
                        class: "flex items-center gap-2",
                        span { class: "label-text", "Synchronize Todoist tasks from" }
                        code { "#{TODOIST_INBOX_PROJECT}" }
                        span { class: "label-text", "as notifications" }
                    }
                    input {
                        r#type: "checkbox",
                        class: "toggle toggle-ghost",
                        oninput: move |event| {
                            on_config_change.call(IntegrationConnectionConfig::Todoist(TodoistConfig {
                                create_notification_from_inbox_task: event.value == "true",
                                ..config.clone()
                            }))
                        },
                        checked: config.create_notification_from_inbox_task
                    }
                }
            }
        }
    }
}
