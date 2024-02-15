#![allow(non_snake_case)]
use dioxus::prelude::*;

use universal_inbox::integration_connection::{
    config::IntegrationConnectionConfig, integrations::slack::SlackConfig,
};

#[component]
pub fn SlackProviderConfiguration<'a>(
    cx: Scope,
    config: SlackConfig,
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
                        "Synchronize Slack stars as notifications"
                    }
                    input {
                        r#type: "checkbox",
                        class: "toggle toggle-ghost",
                        oninput: move |event| {
                            on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                                sync_stars_as_notifications: event.value == "true",
                            }))
                        },
                        checked: config.sync_stars_as_notifications
                    }
                }
            }
        }
    }
}
