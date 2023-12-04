#![allow(non_snake_case)]
use dioxus::prelude::*;

use universal_inbox::integration_connection::integrations::todoist::TodoistConfig;

#[inline_props]
pub fn TodoistProviderConfiguration(cx: Scope, config: TodoistConfig) -> Element {
    render! {
        div {
            class: "flex flex-col",

            div {
                class: "form-control",
                label {
                    class: "cursor-pointer label",
                    span {
                        class: "label-text, text-neutral-content",
                        "Synchronize Todoist tasks"
                    }
                    input {
                        r#type: "checkbox",
                        class: "toggle toggle-primary",
                        disabled: true,
                        checked: config.sync_tasks_enabled
                    }
                }
            }

            div {
                class: "form-control",
                label {
                    class: "cursor-pointer label",
                    span {
                        class: "label-text, text-neutral-content",
                        "Synchronize Todoist tasks from Inbox as notifications"
                    }
                    input {
                        r#type: "checkbox",
                        class: "toggle toggle-primary",
                        disabled: true,
                        checked: true
                    }
                }
            }
        }
    }
}
