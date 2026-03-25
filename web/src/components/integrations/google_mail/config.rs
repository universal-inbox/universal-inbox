#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::integration_connection::{
    config::IntegrationConnectionConfig,
    integrations::google_mail::{GoogleMailConfig, GoogleMailContext},
};

use crate::components::{
    settings_controls::SettingRow,
    ui::{ToggleSize, ToggleSwitch, UISelect, UISelectOption},
};

#[component]
pub fn GoogleMailProviderConfiguration(
    config: ReadSignal<GoogleMailConfig>,
    context: ReadSignal<Option<Option<GoogleMailContext>>>,
    on_config_change: EventHandler<IntegrationConnectionConfig>,
) -> Element {
    let mut selected_label_id = use_signal(|| None);
    use_effect(move || {
        *selected_label_id.write() = Some(config().synced_label.id);
    });

    rsx! {
        SettingRow {
            label: rsx! { "Synchronize Google Mail threads as notification" },
            ToggleSwitch {
                size: ToggleSize::Md,
                checked: config().sync_notifications_enabled,
                onchange: move |new_value: bool| {
                    on_config_change.call(IntegrationConnectionConfig::GoogleMail(GoogleMailConfig {
                        sync_notifications_enabled: new_value,
                        ..config()
                    }))
                },
            }
        }

        SettingRow {
            label: rsx! { "Google Mail label to synchronize" },

            {
                let (options, context_loaded): (Vec<UISelectOption<String>>, bool) = match context() {
                    Some(Some(context)) => (
                        context
                            .labels
                            .iter()
                            .map(|label| UISelectOption::new(label.id.clone(), label.name.clone()))
                            .collect(),
                        true,
                    ),
                    _ => (
                        vec![UISelectOption::new(
                            config().synced_label.id.clone(),
                            config().synced_label.name.clone(),
                        )],
                        false,
                    ),
                };
                rsx! {
                    UISelect::<String> {
                        value: selected_label_id,
                        options,
                        on_change: move |label_id: Option<String>| {
                            if let Some(Some(context)) = context()
                                && let Some(label_id) = label_id
                            {
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
                        placeholder: "Pick a label…".to_string(),
                        disabled: !context_loaded,
                        width: "260px".to_string(),
                        name: "google-mail-label".to_string(),
                    }
                }
            }
        }
    }
}
