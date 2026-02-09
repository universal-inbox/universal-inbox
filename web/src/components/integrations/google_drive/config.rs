#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::integration_connection::{
    config::IntegrationConnectionConfig, integrations::google_drive::GoogleDriveConfig,
};

#[component]
pub fn GoogleDriveProviderConfiguration(
    config: ReadSignal<GoogleDriveConfig>,
    on_config_change: EventHandler<IntegrationConnectionConfig>,
) -> Element {
    rsx! {
        div {
            class: "flex flex-col gap-2",

            div {
                class: "flex items-center gap-2",
                label {
                    class: "label-text cursor-pointer grow text-sm text-base-content",
                    "Synchronize Google Drive comments as notifications"
                }
                div {
                    class: "relative inline-block",
                    input {
                        r#type: "checkbox",
                        class: "switch switch-primary switch-outline peer",
                        oninput: move |event| {
                            on_config_change.call(IntegrationConnectionConfig::GoogleDrive(GoogleDriveConfig {
                                sync_notifications_enabled: event.value() == "true",
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
        }
    }
}
