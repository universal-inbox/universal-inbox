#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::integration_connection::integrations::google_mail::{
    GoogleMailConfig, GoogleMailContext,
};

#[inline_props]
pub fn GoogleMailProviderConfiguration(
    cx: Scope,
    config: GoogleMailConfig,
    context: Option<Option<GoogleMailContext>>,
) -> Element {
    render! {
        div {
            class: "flex flex-col",

            div {
                class: "form-control",
                label {
                    class: "cursor-pointer label",
                    span {
                        class: "label-text, text-neutral-content",
                        "Synchronize Google Mail threads as notification"
                    }
                    input {
                        r#type: "checkbox",
                        class: "toggle toggle-primary",
                        disabled: true,
                        checked: config.sync_notifications_enabled
                    }
                }
            }

            div {
                class: "form-control",
                label {
                    class: "label",
                    span {
                        class: "label-text, text-neutral-content",
                        "Google Mail label to synchronize"
                    }
                    select {
                        class:"select select-sm select-ghost text-xs",
                        disabled: true,

                        if let Some(Some(context)) = context {
                            render! {
                                for label in &context.labels {
                                    render! {
                                        option { selected: *label.id == config.synced_label.id, "{label.name}" }
                                    }
                                }
                            }
                        } else {
                            render! {
                                option { selected: true, "{config.synced_label.name}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
