#![allow(non_snake_case)]

use std::collections::HashMap;

use chrono::{Local, SecondsFormat};
use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsPlug, Icon};

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

pub fn Footer() -> Element {
    let notification_service = use_coroutine_handle::<NotificationCommand>();

    let (message, message_style) = use_memo(move || {
        let Some(integration_connections) = INTEGRATION_CONNECTIONS() else {
            return (None, "");
        };
        let Some(app_config) = APP_CONFIG() else {
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
            if let Some(provider_config) = app_config.integration_providers.get(&c.provider.kind())
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
    })();

    rsx! {
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
                        div {
                            class: "{message_style} w-full rounded p-1.5 flex justify-center text-xs",
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

                div { class: "divider divider-horizontal" }

                match &UI_MODEL.read().notifications_count {
                    Some(Ok(count)) => rsx! {
                        div {
                            class: "tooltip tooltip-left",
                            "data-tip": "{count} notifications loaded",
                            button {
                                class: "badge badge-success text-xs",
                                onclick: move |_| notification_service.send(NotificationCommand::Refresh),
                                "{count}"
                            }
                        }
                    },
                    Some(Err(error)) => rsx! {
                        div {
                            class: "tooltip tooltip-left tooltip-error",
                            "data-tip": "{error}",
                            button {
                                class: "badge badge-error text-xs",
                                onclick: move |_| notification_service.send(NotificationCommand::Refresh),
                                "0"
                            }
                        }
                    },
                    None => rsx! { span { class: "loading loading-ring loading-xs" } },
                }

                div { class: "w-2" }
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

    rsx! {
        if let Some(tooltip) = connection_issue_tooltip {
            div {
                class: "tooltip tooltip-left text-xs text-error",
                "data-tip": "{tooltip}",

                Link {
                    to: Route::SettingsPage {},
                    Icon { class: "w-5 h-5", icon: BsPlug }
                }
            }
        }

        for integration_connection in sorted_notification_connections {
            if let Some(provider_config) = integration_providers.get(&integration_connection.provider.kind()) {
                IntegrationConnectionStatus {
                    connection: integration_connection.clone(),
                    config: provider_config.clone(),
                }
            }
        }

        div { class: "divider divider-horizontal" }

        for integration_connection in sorted_task_connections {
            if let Some(provider_config) = integration_providers.get(&integration_connection.provider.kind()) {
                IntegrationConnectionStatus {
                    connection: integration_connection.clone(),
                    config: provider_config.clone(),
                }
            }
        }
    }
}

#[component]
pub fn IntegrationConnectionStatus(
    connection: IntegrationConnection,
    config: IntegrationProviderStaticConfig,
) -> Element {
    let provider_kind = connection.provider.kind();
    let connection_is_syncing = connection.is_syncing();
    let (connection_style, tooltip) = use_memo(move || match &connection {
        IntegrationConnection {
            status: IntegrationConnectionStatus::Validated,
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
                    format!(
                        "{provider_kind} connection is missing some permissions, please reconnect."
                    ),
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
                .as_ref()
                .map(|message| format!("{provider_kind} connection failed: {message}"))
                .unwrap_or_else(|| "Connection failed".to_string()),
        ),
        IntegrationConnection {
            last_notifications_sync_failure_message: Some(message),
            ..
        }
        | IntegrationConnection {
            last_tasks_sync_failure_message: Some(message),
            ..
        } => (
            "text-error",
            format!("{provider_kind} failed to sync: {message}"),
        ),
        IntegrationConnection { .. } => {
            ("", format!("{provider_kind} connection is not connected."))
        }
    })();

    rsx! {
        div {
            class: "tooltip tooltip-left text-xs",
            "data-tip": "{tooltip}",

            div {
                class: "relative flex items-center justify-center w-6 h-6",
                if connection_is_syncing {
                    span { class: "absolute top-0 left-0 w-6 h-6 loading loading-spinner loading-xs {connection_style} opacity-50" }
                }
                Link {
                    to: Route::SettingsPage {},
                    IntegrationProviderIcon { class: "w-4 h-4 rounded-full {connection_style}", provider_kind: provider_kind },
                }
            }
        }
    }
}
