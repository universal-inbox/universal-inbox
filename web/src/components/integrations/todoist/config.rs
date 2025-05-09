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
            class: "flex flex-col gap-2",

            div {
                class: "flex items-center",
                label {
                    class: "label-text cursor-pointer grow text-sm text-base-content",
                    "Synchronize Todoist tasks"
                }
                div {
                    class: "relative inline-block",
                    input {
                        r#type: "checkbox",
                        class: "switch switch-soft switch-outline switch-sm peer",
                        oninput: move |event| {
                            on_config_change.call(IntegrationConnectionConfig::Todoist(TodoistConfig {
                                sync_tasks_enabled: event.value() == "true",
                                ..config()
                            }))
                        },
                        checked: config().sync_tasks_enabled
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
                class: "flex items-center gap-2",
                label {
                    class: "label-text cursor-pointer grow text-sm text-base-content",
                    "Synchronize Todoist tasks from "
                    code { "#{TODOIST_INBOX_PROJECT}" }
                    " as notifications"
                }
                div {
                    class: "relative inline-block",
                    input {
                        r#type: "checkbox",
                        class: "switch switch-soft switch-outline switch-sm peer",
                        oninput: move |event| {
                            on_config_change.call(IntegrationConnectionConfig::Todoist(TodoistConfig {
                                create_notification_from_inbox_task: event.value() == "true",
                                ..config()
                            }))
                        },
                        checked: config().create_notification_from_inbox_task
                    }
                    span {
                        class: "icon-[tabler--check] text-primary-content absolute start-1 top-1 hidden size-4 peer-checked:block"
                    }
                    span {
                        class: "icon-[tabler--x] text-neutral-content absolute end-1 top-1 block size-4 peer-checked:hidden"
                    }
                }
            }
        }
    }
}
