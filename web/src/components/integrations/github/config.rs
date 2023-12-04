#![allow(non_snake_case)]
use dioxus::prelude::*;

use universal_inbox::integration_connection::integrations::github::GithubConfig;

#[inline_props]
pub fn GithubProviderConfiguration(cx: Scope, config: GithubConfig) -> Element {
    render! {
        div {
            class: "flex flex-col",

            div {
                class: "form-control",
                label {
                    class: "cursor-pointer label",
                    span {
                        class: "label-text, text-neutral-content",
                        "Synchronize Github notifications"
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
