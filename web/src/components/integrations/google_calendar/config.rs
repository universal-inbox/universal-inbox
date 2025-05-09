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
            class: "flex flex-col gap-2",

            div {
                class: "flex items-center gap-2",
                label {
                    class: "label-text cursor-pointer grow text-sm text-base-content",
                    "Synchronize Google Calendar invitation as notification"
                }
                div {
                    class: "relative inline-block",
                    input {
                        r#type: "checkbox",
                        class: "switch switch-soft switch-outline switch-sm peer",
                        oninput: move |event| {
                            on_config_change.call(IntegrationConnectionConfig::GoogleCalendar(GoogleCalendarConfig {
                                sync_event_details_enabled: event.value() == "true",
                            }))
                        },
                        checked: config().sync_event_details_enabled
                    }
                    span {
                        class: "icon-[tabler--check] text-primary-content absolute start-1 top-1 hidden size-4 peer-checked:block"
                    }
                    span {
                        class: "icon-[tabler--x] text-neutral-content absolute end-1 top-1 block size-4 peer-checked:hidden"
                    }
                }
            }
        }
    }
}
