#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::integration_connection::{
    config::IntegrationConnectionConfig,
    integrations::google_mail::{GoogleMailConfig, GoogleMailContext},
};

#[inline_props]
pub fn GoogleMailProviderConfiguration<'a>(
    cx: Scope,
    config: GoogleMailConfig,
    context: Option<Option<GoogleMailContext>>,
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
                        "Synchronize Google Mail threads as notification"
                    }
                    input {
                        r#type: "checkbox",
                        class: "toggle toggle-ghost",
                        oninput: move |event| {
                            on_config_change.call(IntegrationConnectionConfig::GoogleMail(GoogleMailConfig {
                                sync_notifications_enabled: event.value == "true",
                                ..config.clone()
                            }))
                        },
                        checked: config.sync_notifications_enabled
                    }
                }
            }

            div {
                class: "form-control",
                label {
                    class: "label",
                    span {
                        class: "label-text",
                        "Google Mail label to synchronize"
                    }
                    select {
                        class:"select select-sm select-ghost text-xs",
                        oninput: move |event| {
                            if let Some(Some(context)) = context {
                                let selected_label = context.labels.iter().find(|label| label.id == event.value);
                                if let Some(selected_label) = selected_label {
                                    on_config_change.call(IntegrationConnectionConfig::GoogleMail(GoogleMailConfig {
                                        synced_label: selected_label.clone(),
                                        ..config.clone()
                                    }));
                                }
                            }
                        },

                        if let Some(Some(context)) = context {
                            render! {
                                for label in &context.labels {
                                    render! {
                                        option {
                                            selected: *label.id == config.synced_label.id,
                                            value: "{label.id}",
                                            "{label.name}"
                                        }
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
