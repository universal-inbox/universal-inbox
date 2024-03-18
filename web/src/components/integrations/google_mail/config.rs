#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::integration_connection::{
    config::IntegrationConnectionConfig,
    integrations::google_mail::{GoogleMailConfig, GoogleMailContext},
};

use crate::components::floating_label_inputs::FloatingLabelSelect;

#[component]
pub fn GoogleMailProviderConfiguration<'a>(
    cx: Scope,
    config: GoogleMailConfig,
    context: Option<Option<GoogleMailContext>>,
    on_config_change: EventHandler<'a, IntegrationConnectionConfig>,
) -> Element {
    let selected_label_id = use_state(cx, || None);
    let _ = use_memo(cx, config, |config| {
        selected_label_id.set(Some(config.synced_label.id.clone()));
    });

    render! {
        div {
            class: "flex flex-col",

            div {
                class: "form-control",
                label {
                    class: "cursor-pointer label py-1",
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

                    FloatingLabelSelect::<String> {
                        label: None,
                        class: "w-full max-w-xs bg-base-100 rounded",
                        name: "google-mail-label".to_string(),
                        value: selected_label_id.clone(),
                        required: true,
                        on_select: move |label_id| {
                            if let Some(Some(context)) = context {
                                if let Some(label_id) = label_id {
                                    let label = context
                                        .labels
                                        .iter()
                                        .find(|label| label.id == label_id);
                                    if let Some(label) = label {
                                        on_config_change.call(IntegrationConnectionConfig::GoogleMail(GoogleMailConfig {
                                            synced_label: label.clone(),
                                            ..config.clone()
                                        }));
                                    }
                                }
                            }
                        },

                        if let Some(Some(context)) = context {
                            render! {
                                for label in &context.labels {
                                    render! {
                                        option { value: "{label.id}", "{label.name}" }
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
