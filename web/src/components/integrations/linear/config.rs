#![allow(non_snake_case)]
use dioxus::prelude::*;

use universal_inbox::integration_connection::{
    config::IntegrationConnectionConfig, integrations::linear::LinearConfig,
};

#[component]
pub fn LinearProviderConfiguration<'a>(
    cx: Scope,
    config: LinearConfig,
    on_config_change: EventHandler<'a, IntegrationConnectionConfig>,
) -> Element {
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
                            }))
                        },
                        checked: config.sync_notifications_enabled
                    }
                }
            }
        }
    }
}
