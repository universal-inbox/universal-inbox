#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::integration_connection::{
    config::IntegrationConnectionConfig, integrations::google_drive::GoogleDriveConfig,
};

use crate::components::{
    settings_controls::SettingRow,
    ui::{ToggleSize, ToggleSwitch},
};

#[component]
pub fn GoogleDriveProviderConfiguration(
    config: ReadSignal<GoogleDriveConfig>,
    on_config_change: EventHandler<IntegrationConnectionConfig>,
) -> Element {
    rsx! {
        SettingRow {
            label: rsx! { "Synchronize Google Drive comments as notifications" },
            ToggleSwitch {
                size: ToggleSize::Md,
                checked: config().sync_notifications_enabled,
                onchange: move |new_value: bool| {
                    on_config_change.call(IntegrationConnectionConfig::GoogleDrive(GoogleDriveConfig {
                        sync_notifications_enabled: new_value,
                    }))
                },
            }
        }
    }
}
