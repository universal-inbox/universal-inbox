#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::integration_connection::{
    config::IntegrationConnectionConfig,
    integrations::google_mail::{GoogleMailConfig, GoogleMailContext},
};

use crate::components::floating_label_inputs::FloatingLabelSelect;

#[component]
pub fn GoogleMailProviderConfiguration(
    config: ReadOnlySignal<GoogleMailConfig>,
    context: ReadOnlySignal<Option<Option<GoogleMailContext>>>,
    on_config_change: EventHandler<IntegrationConnectionConfig>,
) -> Element {
    let mut selected_label_id = use_signal(|| None);
    let _ = use_memo(move || {
        *selected_label_id.write() = Some(config().synced_label.id);
    });

    rsx! {
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
                                sync_notifications_enabled: event.value() == "true",
                                ..config()
                            }))
                        },
                        checked: config().sync_notifications_enabled
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
                        required: true,
                        on_select: move |label_id| {
                            if let Some(Some(context)) = context() {
                                if let Some(label_id) = label_id {
                                    let label = context
                                        .labels
                                        .iter()
                                        .find(|label| label.id == label_id);
                                    if let Some(label) = label {
                                        on_config_change.call(IntegrationConnectionConfig::GoogleMail(GoogleMailConfig {
                                            synced_label: label.clone(),
                                            ..config()
                                        }));
                                    }
                                }
                            }
                        },

                        if let Some(Some(context)) = context() {
                            for label in &context.labels {
                                option { selected: selected_label_id() == Some(label.id.clone()), value: "{label.id}", "{label.name}" }
                            }
                        } else {
                            option { selected: true, "{config().synced_label.name}" }
                        }
                    }
                }
            }
        }
    }
}
