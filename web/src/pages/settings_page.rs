use dioxus::prelude::*;
use fermi::use_atom_ref;
use log::debug;
use universal_inbox::integration_connection::{IntegrationConnection, IntegrationProviderKind};

use crate::{
    components::integrations_panel::integrations_panel,
    config::APP_CONFIG,
    services::integration_connection_service::{
        IntegrationConnectionCommand, INTEGRATION_CONNECTIONS,
    },
};

pub fn settings_page(cx: Scope) -> Element {
    let app_config_ref = use_atom_ref(cx, APP_CONFIG);
    let integration_connections_ref = use_atom_ref(cx, INTEGRATION_CONNECTIONS);
    let integration_connection_service =
        use_coroutine_handle::<IntegrationConnectionCommand>(cx).unwrap();

    debug!("Rendering settings page");
    use_future(cx, (), |()| {
        to_owned![integration_connection_service];
        async move {
            integration_connection_service.send(IntegrationConnectionCommand::Refresh);
        }
    });

    if let Some(app_config) = app_config_ref.read().as_ref() {
        cx.render(rsx!(
            div {
                class: "w-full flex-1 overflow-auto",

                div {
                    class: "container mx-auto",

                    integrations_panel {
                        integration_providers: app_config.integration_providers.clone(),
                        integration_connections: integration_connections_ref.read().clone(),
                        on_connect: |(provider_kind, connection): (IntegrationProviderKind, Option<&IntegrationConnection>)| {
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
                        on_disconnect: |connection: &IntegrationConnection| {
                            integration_connection_service.send(
                                IntegrationConnectionCommand::DisconnectIntegrationConnection(connection.id)
                            );
                        },
                        on_reconnect: |connection: &IntegrationConnection| {
                            integration_connection_service.send(
                                IntegrationConnectionCommand::ReconnectIntegrationConnection(connection.clone())
                            );
                        },
                    }
                }
            }
        ))
    } else {
        cx.render(rsx!(div {
            class: "h-full flex justify-center items-center",

            "Universal Inbox configuration has not been loaded."
        }))
    }
}
