#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::integration_connection::{
    config::IntegrationConnectionConfig, integrations::notion::NotionConfig,
};

use crate::components::{
    settings_controls::SettingRow,
    ui::{ToggleSize, ToggleSwitch},
};

#[component]
pub fn NotionProviderConfiguration(
    config: ReadSignal<NotionConfig>,
    on_config_change: EventHandler<IntegrationConnectionConfig>,
) -> Element {
    rsx! {
        p {
            class: "py-1.5 text-[10.5px] text-ui-base-muted",
            "Universal Inbox only sees the pages you share with it in Notion's page picker. \
             Share your team spaces to get their comments and mentions."
        }
        SettingRow {
            label: rsx! { "Synchronize Notion comments as notifications" },
            description: "Comments in the discussions you take part in or are mentioned in",
            ToggleSwitch {
                size: ToggleSize::Md,
                checked: config().sync_comments_enabled,
                onchange: move |new_value: bool| {
                    on_config_change.call(IntegrationConnectionConfig::Notion(NotionConfig {
                        sync_comments_enabled: new_value,
                        ..config()
                    }))
                },
            }
        }
        SettingRow {
            label: rsx! { "Synchronize Notion page mentions as notifications" },
            description: "Mentions of you in the content of a page",
            ToggleSwitch {
                size: ToggleSize::Md,
                checked: config().sync_mentions_enabled,
                onchange: move |new_value: bool| {
                    on_config_change.call(IntegrationConnectionConfig::Notion(NotionConfig {
                        sync_mentions_enabled: new_value,
                        ..config()
                    }))
                },
            }
        }
    }
}
