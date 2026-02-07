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
    use_effect(move || {
        *selected_label_id.write() = Some(config().synced_label.id);
    });

    rsx! {
        div {
            class: "flex flex-col gap-2",

            div {
                class: "flex items-center gap-2",
                label {
                    class: "label-text cursor-pointer grow text-sm text-base-content",
                    "Synchronize Google Mail threads as notification"
                }
                div {
                    class: "relative inline-block",
                    input {
                        r#type: "checkbox",
                        class: "switch switch-primary switch-outline peer",
                        oninput: move |event| {
                            on_config_change.call(IntegrationConnectionConfig::GoogleMail(GoogleMailConfig {
                                sync_notifications_enabled: event.value() == "true",
                                ..config()
                            }))
                        },
                        checked: config().sync_notifications_enabled
                    }
                    span {
                        class: "icon-[tabler--check] text-primary absolute start-1 top-1 hidden size-4 peer-checked:block"
                    }
                    span {
                        class: "icon-[tabler--x] text-neutral absolute end-1 top-1 block size-4 peer-checked:hidden"
                    }
                }
            }

            div {
                class: "flex items-center gap-2",
                label {
                    class: "label-text cursor-pointer grow text-sm text-base-content",
                    "Google Mail label to synchronize"
                }

                FloatingLabelSelect::<String> {
                    label: None,
                    class: "max-w-xs",
                    name: "google-mail-label".to_string(),
                    required: true,
                    default_value: selected_label_id(),
                    on_select: move |label_id| {
                        if let Some(Some(context)) = context()
                            && let Some(label_id) = label_id {
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
