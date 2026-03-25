#![allow(non_snake_case)]
use dioxus::prelude::*;

use universal_inbox::integration_connection::{
    config::IntegrationConnectionConfig, integrations::github::GithubConfig,
};

use crate::components::{
    settings_controls::SettingRow,
    ui::{ToggleSize, ToggleSwitch},
};

#[component]
pub fn GithubProviderConfiguration(
    config: GithubConfig,
    on_config_change: EventHandler<IntegrationConnectionConfig>,
) -> Element {
    rsx! {
        SettingRow {
            label: rsx! { "Synchronize Github notifications" },
            ToggleSwitch {
                size: ToggleSize::Md,
                checked: config.sync_notifications_enabled,
                onchange: move |new_value: bool| {
                    on_config_change.call(IntegrationConnectionConfig::Github(GithubConfig {
                        sync_notifications_enabled: new_value,
                    }))
                },
            }
        }
    }
}
