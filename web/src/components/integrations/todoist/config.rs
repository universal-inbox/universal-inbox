#![allow(non_snake_case)]
use dioxus::prelude::*;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::todoist::TodoistConfig,
    },
    task::integrations::todoist::TODOIST_INBOX_PROJECT,
};

#[component]
pub fn TodoistProviderConfiguration(
    config: ReadOnlySignal<TodoistConfig>,
    on_config_change: EventHandler<IntegrationConnectionConfig>,
) -> Element {
    rsx! {
        div {
            class: "flex flex-col",

            fieldset {
                class: "fieldset",
                label {
                    class: "fieldset-label cursor-pointer py-1 text-sm text-base-content",
                    span {
                        class: "label-text grow",
                        "Synchronize Todoist tasks"
                    }
                    input {
                        r#type: "checkbox",
                        class: "toggle toggle-ghost",
                        oninput: move |event| {
                            on_config_change.call(IntegrationConnectionConfig::Todoist(TodoistConfig {
                                sync_tasks_enabled: event.value() == "true",
                                ..config()
                            }))
                        },
                        checked: config().sync_tasks_enabled
                    }
                }
            }

            fieldset {
                class: "fieldset",
                label {
                    class: "fieldset-label cursor-pointer py-1 text-sm text-base-content",
                    span {
                        class: "label-text grow",
                        "Synchronize Todoist tasks from "
                        code { "#{TODOIST_INBOX_PROJECT}" }
                        " as notifications"
                    }
                    input {
                        r#type: "checkbox",
                        class: "toggle toggle-ghost",
                        oninput: move |event| {
                            on_config_change.call(IntegrationConnectionConfig::Todoist(TodoistConfig {
                                create_notification_from_inbox_task: event.value() == "true",
                                ..config()
                            }))
                        },
                        checked: config().create_notification_from_inbox_task
                    }
                }
            }
        }
    }
}
