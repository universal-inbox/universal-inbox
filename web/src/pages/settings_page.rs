#![allow(non_snake_case)]

use dioxus::prelude::*;

use log::debug;

use universal_inbox::integration_connection::{
    config::IntegrationConnectionConfig, provider::IntegrationProviderKind, IntegrationConnection,
};

use crate::{
    components::{integrations_panel::IntegrationsPanel, spinner::Spinner},
    config::APP_CONFIG,
    model::UI_MODEL,
    services::integration_connection_service::{
        IntegrationConnectionCommand, INTEGRATION_CONNECTIONS,
    },
};

pub fn SettingsPage() -> Element {
    let integration_connection_service = use_coroutine_handle::<IntegrationConnectionCommand>();

    debug!("Rendering settings page");

    let _ = use_resource(move || {
        to_owned![integration_connection_service];

        async move {
            integration_connection_service.send(IntegrationConnectionCommand::Refresh);
        }
    });

    if let Some(app_config) = APP_CONFIG.read().as_ref() {
        if let Some(integration_connections) = INTEGRATION_CONNECTIONS.read().as_ref() {
            return rsx! {
                div {
                    class: "h-full mx-auto flex flex-row px-4",

                    div {
                        class: "h-full w-full overflow-auto scroll-auto px-2",

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
    }

    rsx! {
        div {
            class: "h-full flex justify-center items-center",

            Spinner {}
            "Loading Universal Inbox settings..."
        }
    }
}
