#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::integration_connection::{
    config::IntegrationConnectionConfig, integrations::google_calendar::GoogleCalendarConfig,
};

use crate::components::{
    settings_controls::SettingRow,
    ui::{ToggleSize, ToggleSwitch},
};

#[component]
pub fn GoogleCalendarProviderConfiguration(
    config: ReadSignal<GoogleCalendarConfig>,
    on_config_change: EventHandler<IntegrationConnectionConfig>,
) -> Element {
    rsx! {
        SettingRow {
            label: rsx! { "Synchronize Google Calendar invitation as notification" },
            ToggleSwitch {
                size: ToggleSize::Md,
                checked: config().sync_event_details_enabled,
                onchange: move |new_value: bool| {
                    on_config_change.call(IntegrationConnectionConfig::GoogleCalendar(GoogleCalendarConfig {
                        sync_event_details_enabled: new_value,
                    }))
                },
            }
        }
    }
}
