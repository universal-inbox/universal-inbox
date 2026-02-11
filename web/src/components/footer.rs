#![allow(non_snake_case)]

use std::collections::HashMap;

use chrono::{Local, SecondsFormat};
use dioxus::prelude::*;
use dioxus_free_icons::{Icon, icons::bs_icons::BsPlug};

use universal_inbox::{
    IntegrationProviderStaticConfig,
    integration_connection::{
        IntegrationConnection, IntegrationConnectionStatus as ConnectionStatus,
        provider::IntegrationProviderKind,
    },
};

use crate::{
    components::{
        flyonui::tooltip::{Tooltip, TooltipPlacement},
        integrations::icons::IntegrationProviderIcon,
    },
    config::APP_CONFIG,
    route::Route,
    services::integration_connection_service::INTEGRATION_CONNECTIONS,
};

pub fn Footer() -> Element {
    let (message, message_style, container_style) = use_memo(move || {
        let Some(integration_connections) = INTEGRATION_CONNECTIONS() else {
            return (None, "", "max-sm:hidden");
        };
        let Some(app_config) = APP_CONFIG() else {
            return (None, "", "max-sm:hidden");
        };
        let has_connection_issue = integration_connections.iter().any(|c| c.is_failing());
        if has_connection_issue {
            return (
                Some("Some integrations have issues, please reconnect them."),
                "bg-error text-error-content",
                "",
            );
        };
        let has_degraded_sync = integration_connections.iter().any(|c| c.is_sync_degraded());
        if has_degraded_sync {
            return (
                Some("Some integrations are experiencing sync issues. Retrying automatically."),
                "bg-warning text-warning-content",
                "",
            );
        };
        let has_missing_permission = integration_connections.iter().any(|c| {
            if let Some(provider_config) = app_config.integration_providers.get(&c.provider.kind())
            {
                c.is_connected() && !c.has_oauth_scopes(&provider_config.required_oauth_scopes)
            } else {
                false
            }
        });
        if has_missing_permission {
            (
                Some("Some integrations are missing permissions, please reconnect them."),
                "bg-warning text-warning-content",
                "",
            )
        } else {
            (None, "", "max-sm:hidden")
        }
    })();

    rsx! {
        footer {
            class: "w-full max-h-20",

            hr { class: "text-gray-200" }
            div {
                class: "w-full flex max-sm:flex-col gap-2 p-1 justify-end items-center",

                div {
                    class: "text-xs text-base-content/50 pointer-coarse:hidden",
                    "Press "
                    kbd { class: "kbd kbd-xs", "?" }
                    " to display keyboard shortcuts"
                }

                div {
                    class: "grow {container_style}",

                    if let Some(message) = message {
                        div {
                            class: "{message_style} w-full rounded-sm p-1.5 flex justify-center text-xs",
                            Link {
                                to: Route::SettingsPage {},
                                span { "{message}" }
                            }
                        }
                    }
                }

                if let Some(integration_connections) = INTEGRATION_CONNECTIONS().as_ref() {
                    if let Some(app_config) = APP_CONFIG().as_ref() {
                        IntegrationConnectionsStatus {
                            integration_connections: integration_connections.clone(),
                            integration_providers: app_config.integration_providers.clone()
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn IntegrationConnectionsStatus(
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
    let mut sorted_notification_connections = integration_connections
        .iter()
        .filter(|c| c.provider.kind().is_notification_service())
        .collect::<Vec<&IntegrationConnection>>();
    sorted_notification_connections.sort_by(|a, b| {
        a.provider
            .kind()
            .to_string()
            .cmp(&b.provider.kind().to_string())
    });

    let mut sorted_task_connections = integration_connections
        .iter()
        .filter(|c| c.provider.kind().is_task_service())
        .collect::<Vec<&IntegrationConnection>>();
    sorted_task_connections.sort_by(|a, b| {
        a.provider
            .kind()
            .to_string()
            .cmp(&b.provider.kind().to_string())
    });

    let mut sorted_utils_connections = integration_connections
        .iter()
        .filter(|c| {
            !c.provider.kind().is_notification_service() && !c.provider.kind().is_task_service()
        })
        .collect::<Vec<&IntegrationConnection>>();
    sorted_utils_connections.sort_by(|a, b| {
        a.provider
            .kind()
            .to_string()
            .cmp(&b.provider.kind().to_string())
    });

    rsx! {
        div {
            class: "flex divide-x divide-base-content/25 items-center",

            if let Some(tooltip) = connection_issue_tooltip {
                div {
                    class: "px-2",
                    Tooltip {
                        tooltip_class: "tooltip-error",
                        text: "{tooltip}",
                        placement: TooltipPlacement::Left,

                        Link {
                            class: "tooltip-toggle text-error",
                            to: Route::SettingsPage {},
                            Icon { class: "w-5 h-5", icon: BsPlug }
                        }
                    }
                }
            }

            div {
                class: "px-2",
                for integration_connection in sorted_notification_connections {
                    if let Some(provider_config) = integration_providers.get(&integration_connection.provider.kind()) {
                        if provider_config.is_enabled {
                            IntegrationConnectionStatus {
                                connection: integration_connection.clone(),
                                config: provider_config.clone(),
                            }
                        }
                    }
                }
            }

            div {
                class: "px-2",
                for integration_connection in sorted_task_connections {
                    if let Some(provider_config) = integration_providers.get(&integration_connection.provider.kind()) {
                        if provider_config.is_enabled {
                            IntegrationConnectionStatus {
                                connection: integration_connection.clone(),
                                config: provider_config.clone(),
                            }
                        }
                    }
                }
            }

            div {
                class: "px-2",
                for integration_connection in sorted_utils_connections {
                    if let Some(provider_config) = integration_providers.get(&integration_connection.provider.kind()) {
                        if provider_config.is_enabled {
                            IntegrationConnectionStatus {
                                connection: integration_connection.clone(),
                                config: provider_config.clone(),
                                icon_class: "w-6 h-6",
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn IntegrationConnectionStatus(
    connection: IntegrationConnection,
    config: IntegrationProviderStaticConfig,
    icon_class: Option<&'static str>,
) -> Element {
    let icon_style = icon_class.unwrap_or("w-4 h-4");
    let provider_kind = connection.provider.kind();
    let connection_is_syncing = connection.is_syncing();
    let (connection_style, tooltip_style, tooltip) = use_memo(move || match &connection {
        IntegrationConnection {
            status: ConnectionStatus::Validated,
            last_notifications_sync_started_at: notifs_started_at,
            last_tasks_sync_started_at: tasks_started_at,
            last_notifications_sync_failure_message: None,
            last_tasks_sync_failure_message: None,
            ..
        } => {
            if connection.has_oauth_scopes(&config.required_oauth_scopes) {
                let started_at = match (notifs_started_at, tasks_started_at) {
                    (Some(notifs_started_at), Some(tasks_started_at)) => {
                        Some(notifs_started_at.max(tasks_started_at))
                    }
                    (Some(notifs_started_at), None) => Some(notifs_started_at),
                    (None, Some(tasks_started_at)) => Some(tasks_started_at),
                    _ => None,
                };
                (
                    "text-success",
                    "tooltip-success",
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
                    "tooltip-warning",
                    format!(
                        "{provider_kind} connection is missing some permissions, please reconnect."
                    ),
                )
            }
        }
        IntegrationConnection {
            status: ConnectionStatus::Failing,
            failure_message: message,
            ..
        } => (
            "text-error",
            "tooltip-error",
            message
                .as_ref()
                .map(|message| format!("{provider_kind} connection failed: {message}"))
                .unwrap_or_else(|| "Connection failed".to_string()),
        ),
        IntegrationConnection {
            status: ConnectionStatus::Validated,
            last_notifications_sync_failure_message: Some(message),
            ..
        }
        | IntegrationConnection {
            status: ConnectionStatus::Validated,
            last_tasks_sync_failure_message: Some(message),
            ..
        } => (
            "text-warning",
            "tooltip-warning",
            format!("{provider_kind} sync is degraded: {message}"),
        ),
        IntegrationConnection { .. } => (
            "",
            "",
            format!("{provider_kind} connection is not connected."),
        ),
    })();

    rsx! {
        Tooltip {
            tooltip_class: "{tooltip_style}",
            text: "{tooltip}",
            placement: TooltipPlacement::Left,

            div {
                class: "relative flex items-center justify-center w-6 h-6 tooltip-toggle",
                if connection_is_syncing {
                    span { class: "absolute top-0 left-0 w-6 h-6 loading loading-spinner loading-xs {connection_style} opacity-50" }
                }
                Link {
                    to: Route::SettingsPage {},
                    IntegrationProviderIcon { class: "{icon_style} rounded-full {connection_style}", provider_kind: provider_kind },
                }
            }
        }
    }
}
