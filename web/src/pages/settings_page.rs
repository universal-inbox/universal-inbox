#![allow(non_snake_case)]

use dioxus::prelude::*;

use log::debug;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, config::IntegrationConnectionConfig,
        provider::IntegrationProviderKind,
    },
    user::UserPreferencesPatch,
};

use crate::{
    components::{
        floating_label_inputs::FloatingLabelSelect, integrations_panel::IntegrationsPanel,
        loading::Loading,
    },
    config::APP_CONFIG,
    model::{LoadState, UI_MODEL},
    services::{
        integration_connection_service::{
            INTEGRATION_CONNECTIONS, IntegrationConnectionCommand,
            TASK_SERVICE_INTEGRATION_CONNECTIONS,
        },
        user_preferences_service::{USER_PREFERENCES, UserPreferencesCommand},
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
        let show_preferences = matches!(
            &*TASK_SERVICE_INTEGRATION_CONNECTIONS.read(),
            LoadState::Loaded(connections) if connections.len() >= 2
        );

        let default_task_manager = USER_PREFERENCES
            .read()
            .as_ref()
            .and_then(|p| p.default_task_manager_provider_kind);

        return rsx! {
            div {
                class: "h-full mx-auto flex flex-row px-4",

                div {
                    class: "h-full w-full overflow-y-auto scroll-y-auto px-2",

                    div {
                        class: "flex flex-col w-auto gap-4 p-8",

                        if show_preferences {
                            div {
                                class: "flex items-center gap-4 w-full",
                                div {
                                    class: "leading-none relative shrink-0",
                                    span { class: "w-0 h-12 inline-block align-middle" }
                                    span { class: "relative text-2xl", "General preferences" }
                                }
                                div { class: "divider grow" }
                            }

                            div {
                                class: "card w-full bg-base-200",
                                div {
                                    class: "card-body text-sm",
                                    div {
                                        class: "flex items-center gap-2",
                                        label {
                                            class: "label-text cursor-pointer grow text-sm text-base-content",
                                            "Default task manager for quick actions"
                                        }
                                        FloatingLabelSelect::<IntegrationProviderKind> {
                                            label: None,
                                            class: "max-w-xs",
                                            name: "default-task-manager-input".to_string(),
                                            default_value: default_task_manager.map(|p| p.to_string()).unwrap_or_default(),
                                            on_select: move |provider_kind: Option<IntegrationProviderKind>| {
                                                user_preferences_service.send(
                                                    UserPreferencesCommand::Patch(UserPreferencesPatch {
                                                        default_task_manager_provider_kind: Some(provider_kind),
                                                    })
                                                );
                                            },

                                            option { selected: default_task_manager == Some(IntegrationProviderKind::Todoist), value: "Todoist", "Todoist" }
                                            option { selected: default_task_manager == Some(IntegrationProviderKind::TickTick), value: "TickTick", "TickTick" }
                                        }
                                    }
                                }
                            }
                        }
                    }

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
