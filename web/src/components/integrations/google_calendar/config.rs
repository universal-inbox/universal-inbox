#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::integration_connection::{
    config::IntegrationConnectionConfig, integrations::google_calendar::GoogleCalendarConfig,
};

#[component]
pub fn GoogleCalendarProviderConfiguration(
    config: ReadOnlySignal<GoogleCalendarConfig>,
    on_config_change: EventHandler<IntegrationConnectionConfig>,
) -> Element {
    rsx! {
        div {
            class: "flex flex-col",

            fieldset {
                class: "fieldset",
                label {
                    class: "fieldset-label cursor-pointer py-1 text-sm text-base-content",
                    span {
                        class: "label-text grow",
                        "Synchronize Google Calendar invitation as notification"
                    }
                    input {
                        r#type: "checkbox",
                        class: "toggle toggle-ghost",
                        oninput: move |event| {
                            on_config_change.call(IntegrationConnectionConfig::GoogleCalendar(GoogleCalendarConfig {
                                sync_event_details_enabled: event.value() == "true",
                            }))
                        },
                        checked: config().sync_event_details_enabled
                    }
                }
            }
        }
    }
}
