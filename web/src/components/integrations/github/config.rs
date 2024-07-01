#![allow(non_snake_case)]
use dioxus::prelude::*;

use universal_inbox::integration_connection::{
    config::IntegrationConnectionConfig, integrations::github::GithubConfig,
};

#[component]
pub fn GithubProviderConfiguration(
    config: GithubConfig,
    on_config_change: EventHandler<IntegrationConnectionConfig>,
) -> Element {
    rsx! {
        div {
            class: "flex flex-col",

            div {
                class: "form-control",
                label {
                    class: "cursor-pointer label py-1",
                    span {
                        class: "label-text",
                        "Synchronize Github notifications"
                    }
                    input {
                        r#type: "checkbox",
                        class: "toggle toggle-ghost",
                        oninput: move |event| {
                            on_config_change.call(IntegrationConnectionConfig::Github(GithubConfig {
                                sync_notifications_enabled: event.value() == "true",
                            }))
                        },
                        checked: config.sync_notifications_enabled
                    }
                }
            }
        }
    }
}
