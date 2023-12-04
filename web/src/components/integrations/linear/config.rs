#![allow(non_snake_case)]
use dioxus::prelude::*;

use universal_inbox::integration_connection::integrations::linear::LinearConfig;

#[inline_props]
pub fn LinearProviderConfiguration(cx: Scope, config: LinearConfig) -> Element {
    render! {
        div {
            class: "flex flex-col",

            div {
                class: "form-control",
                label {
                    class: "cursor-pointer label",
                    span {
                        class: "label-text, text-neutral-content",
                        "Synchronize Linear notifications"
                    }
                    input {
                        r#type: "checkbox",
                        class: "toggle toggle-primary",
                        disabled: true,
                        checked: config.sync_notifications_enabled
                    }
                }
            }
        }
    }
}
