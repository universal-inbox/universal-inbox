#![allow(non_snake_case)]

use dioxus::prelude::*;

use log::debug;

use universal_inbox::integration_connection::{
    IntegrationConnection, config::IntegrationConnectionConfig, provider::IntegrationProviderKind,
};

use crate::{
    components::{integrations_panel::IntegrationsPanel, loading::Loading},
    config::APP_CONFIG,
    model::UI_MODEL,
    services::{
        integration_connection_service::{INTEGRATION_CONNECTIONS, IntegrationConnectionCommand},
        user_preferences_service::UserPreferencesCommand,
    },
};

pub fn SettingsPage() -> Element {
    let integration_connection_service = use_coroutine_handle::<IntegrationConnectionCommand>();
    let user_preferences_service = use_coroutine_handle::<UserPreferencesCommand>();

    debug!("Rendering settings page");

    let _resource = use_resource(move || {
        to_owned![integration_connection_service, user_preferences_service];

        async move {
            integration_connection_service.send(IntegrationConnectionCommand::Refresh);
            user_preferences_service.send(UserPreferencesCommand::Refresh);
        }
    });

    if let Some(app_config) = APP_CONFIG.read().as_ref()
        && let Some(integration_connections) = INTEGRATION_CONNECTIONS.read().as_ref()
    {
        return rsx! {
            div {
                class: "flex-1 overflow-y-auto bg-ui-base-200",

                div {
                    // Settings shell: 16/20/40 padding, 740px max width, fade-up
                    // on mount. The `animate-detail-fade` utility maps to the
                    // `detail-fade` keyframe via the `--animate-detail-fade`
                    // @theme entry; same animation timing as the legacy
                    // `.settings-container` rule.
                    class: "px-5 pt-4 pb-10 max-w-[740px] mx-auto animate-detail-fade",

                    IntegrationsPanel {
                        ui_model: UI_MODEL.signal(),
                        integration_providers: app_config.integration_providers.clone(),
                        integration_connections: integration_connections.clone(),
                        on_connect: move |(provider_kind, connection): (IntegrationProviderKind, Option<IntegrationConnection>)| {
                            if let Some(connection) = connection {
                                integration_connection_service.send(
                                    IntegrationConnectionCommand::AuthenticateIntegrationConnection(connection.clone())
                                );
                            } else {
                                integration_connection_service.send(
                                    IntegrationConnectionCommand::CreateIntegrationConnection(provider_kind)
                                );
                            }
                        },
                        on_disconnect: move |connection: IntegrationConnection| {
                            integration_connection_service.send(
                                IntegrationConnectionCommand::DisconnectIntegrationConnection(connection.id)
                            );
                        },
                        on_reconnect: move |connection: IntegrationConnection| {
                            integration_connection_service.send(
                                IntegrationConnectionCommand::ReconnectIntegrationConnection(connection.clone())
                            );
                        },
                        on_config_change: move |(connection, config): (IntegrationConnection, IntegrationConnectionConfig)| {
                            integration_connection_service.send(
                                IntegrationConnectionCommand::UpdateIntegrationConnectionConfig(connection.clone(), config)
                            );
                        },
                    }
                }
            }
        };
    }

    rsx! { Loading { label: "Loading Universal Inbox settings..." } }
}
