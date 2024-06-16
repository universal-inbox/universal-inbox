#![allow(non_snake_case)]

use std::collections::HashMap;

use chrono::{Local, SecondsFormat};
use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsPlug, Icon};
use dioxus_router::prelude::*;
use fermi::use_atom_ref;

use universal_inbox::{
    integration_connection::{
        provider::IntegrationProviderKind, IntegrationConnection, IntegrationConnectionStatus,
    },
    IntegrationProviderStaticConfig,
};

use crate::{
    components::integrations::icons::IntegrationProviderIcon,
    config::APP_CONFIG,
    model::UI_MODEL,
    route::Route,
    services::{
        integration_connection_service::INTEGRATION_CONNECTIONS,
        notification_service::NotificationCommand,
    },
};

pub fn Footer(cx: Scope) -> Element {
    let app_config_ref = use_atom_ref(cx, &APP_CONFIG);
    let integration_connections_ref = use_atom_ref(cx, &INTEGRATION_CONNECTIONS);
    let ui_model_ref = use_atom_ref(cx, &UI_MODEL);
    let notification_service = use_coroutine_handle::<NotificationCommand>(cx).unwrap();

    let app_config = app_config_ref.read();
    let integration_connections = integration_connections_ref.read();
    let (message, message_style) = use_memo(
        cx,
        &(integration_connections.clone(), app_config.clone()),
        |(integration_connections, app_config)| {
            let Some(integration_connections) = integration_connections else {
                return (None, "");
            };
            let Some(app_config) = app_config else {
                return (None, "");
            };
            let has_connection_issue = integration_connections.iter().any(|c| c.is_failing());
            if has_connection_issue {
                return (
                    Some("Some integrations have issues, please reconnect them."),
                    "bg-error",
                );
            };
            let has_missing_permission = integration_connections.iter().any(|c| {
                if let Some(provider_config) =
                    app_config.integration_providers.get(&c.provider.kind())
                {
                    c.is_connected() && !c.has_oauth_scopes(&provider_config.required_oauth_scopes)
                } else {
                    true
                }
            });
            if has_missing_permission {
                (
                    Some("Some integrations are missing permissions, please reconnect them."),
                    "bg-warning",
                )
            } else {
                (None, "")
            }
        },
    );

    let app_config = app_config.as_ref()?;

    render! {
        footer {
            class: "w-full",

            hr {}
            div {
                class: "w-full flex gap-2 p-1 justify-end items-center",

                div {
                    class: "text-xs text-gray-400",
                    "Press "
                    kbd { class: "kbd kbd-xs", "?" }
                    " to display keyboard shortcuts"
                }

                div {
                    class: "grow",

                    if let Some(message) = message {
                        render! {
                            div {
                                class: "{message_style} w-full rounded p-1.5 flex justify-center text-xs",
                                Link {
                                    to: Route::SettingsPage {},
                                    span { message }
                                }
                            }
                        }
                    }
                }

                if let Some(integration_connections) = integration_connections_ref.read().as_ref() {
                    render! {
                        IntegrationConnectionsStatus {
                            integration_connections: integration_connections.clone(),
                            integration_providers: app_config.integration_providers.clone()
                        }
                    }
                }

                div { class: "divider divider-horizontal" }

                match &ui_model_ref.read().notifications_count {
                    Some(Ok(count)) => render! {
                        div {
                            class: "tooltip tooltip-left",
                            "data-tip": "{count} notifications loaded",
                            button {
                                class: "badge badge-success text-xs",
                                onclick: |_| notification_service.send(NotificationCommand::Refresh),
                                "{count}"
                            }
                        }
                    },
                    Some(Err(error)) => render! {
                        div {
                            class: "tooltip tooltip-left tooltip-error",
                            "data-tip": "{error}",
                            button {
                                class: "badge badge-error text-xs",
                                onclick: |_| notification_service.send(NotificationCommand::Refresh),
                                "0"
                            }
                        }
                    },
                    None => render! { span { class: "loading loading-ring loading-xs" } },
                }

                div { class: "w-2" }
            }
        }
    }
}

#[component]
pub fn IntegrationConnectionsStatus(
    cx: Scope,
    integration_connections: Vec<IntegrationConnection>,
    integration_providers: HashMap<IntegrationProviderKind, IntegrationProviderStaticConfig>,
) -> Element {
    let connection_issue_tooltip = if !integration_connections.iter().any(|c| c.is_connected()) {
        Some("No integration connected")
    } else if !integration_connections
        .iter()
        .any(|c| c.is_connected_task_service())
    {
        Some("No task management integration connected")
    } else {
        None
    };

    render! {
        if let Some(tooltip) = connection_issue_tooltip {
            render! {
                div {
                    class: "tooltip tooltip-left text-xs text-error",
                    "data-tip": "{tooltip}",

                    Link {
                        to: Route::SettingsPage {},
                        Icon { class: "w-5 h-5", icon: BsPlug }
                    }
                }
            }
        }

        for integration_connection in (integration_connections.iter().filter(|c| c.provider.kind().is_notification_service() )) {
            if let Some(provider_config) = integration_providers.get(&integration_connection.provider.kind()) {
                render! {
                    IntegrationConnectionStatus {
                        connection: integration_connection.clone(),
                        config: provider_config.clone(),
                    }
                }
            } else {
                None
            }
        }

        div { class: "divider divider-horizontal" }

        for integration_connection in (integration_connections.iter().filter(|c| c.provider.kind().is_task_service() )) {
            if let Some(provider_config) = integration_providers.get(&integration_connection.provider.kind()) {
                render! {
                    IntegrationConnectionStatus {
                        connection: integration_connection.clone(),
                        config: provider_config.clone(),
                    }
                }
            } else {
                None
            }
        }
    }
}

#[component]
pub fn IntegrationConnectionStatus(
    cx: Scope,
    connection: IntegrationConnection,
    config: IntegrationProviderStaticConfig,
) -> Element {
    let provider_kind = connection.provider.kind();
    let (connection_style, tooltip) = use_memo(cx, &connection.clone(), |connection| {
        match connection {
            IntegrationConnection {
                status: IntegrationConnectionStatus::Validated,
                last_sync_started_at: started_at,
                last_sync_failure_message: None,
                ..
            } => {
                if connection.has_oauth_scopes(&config.required_oauth_scopes) {
                    (
                        "text-success",
                        started_at
                            .map(|started_at| {
                                format!(
                                    "{provider_kind} successfully synced at {}",
                                    started_at
                                        .with_timezone(&Local)
                                        .to_rfc3339_opts(SecondsFormat::Secs, true)
                                )
                            })
                            .unwrap_or_else(|| format!("{provider_kind} successfully synced")),
                    )
                } else {
                    (
                "text-warning",
                format!("{provider_kind} connection is missing some permissions, please reconnect."),
            )
                }
            }
            IntegrationConnection {
                status: IntegrationConnectionStatus::Failing,
                failure_message: message,
                ..
            } => (
                "text-error",
                message
                    .map(|message| format!("{provider_kind} connection failed: {message}"))
                    .unwrap_or_else(|| "Connection failed".to_string()),
            ),
            IntegrationConnection {
                last_sync_failure_message: Some(message),
                ..
            } => (
                "text-error",
                format!("{provider_kind} failed to sync: {message}"),
            ),
            IntegrationConnection { .. } => {
                ("", format!("{provider_kind} connection is not connected."))
            }
        }
    });

    render! {
        div {
            class: "tooltip tooltip-left text-xs",
            "data-tip": "{tooltip}",

            Link {
                to: Route::SettingsPage {},
                IntegrationProviderIcon { class: "w-4 h-4 {connection_style}", provider_kind: provider_kind },
            }
        }
    }
}
