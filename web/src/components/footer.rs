#![allow(non_snake_case)]

use chrono::{Local, SecondsFormat};
use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsPlug, Icon};
use dioxus_router::prelude::*;
use fermi::use_atom_ref;

use universal_inbox::integration_connection::{
    provider::IntegrationProviderKind, IntegrationConnection, IntegrationConnectionStatus,
};

use crate::{
    components::{
        icons::{GoogleMail, Linear, Todoist},
        integrations::github::icons::Github,
    },
    model::UI_MODEL,
    route::Route,
    services::{
        integration_connection_service::INTEGRATION_CONNECTIONS,
        notification_service::NotificationCommand,
    },
};

pub fn Footer(cx: Scope) -> Element {
    let integration_connections_ref = use_atom_ref(cx, &INTEGRATION_CONNECTIONS);
    let ui_model_ref = use_atom_ref(cx, &UI_MODEL);
    let notification_service = use_coroutine_handle::<NotificationCommand>(cx).unwrap();

    render! {
        footer {
            class: "w-full",

            hr {}
            div {
                class: "w-full flex gap-2 p-2 justify-end items-center",

                div {
                    class: "grow text-xs text-gray-400",
                    "Press "
                    kbd { class: "kbd kbd-xs", "?" }
                    " to display keyboard shortcuts"
                }

                if let Some(integration_connections) = integration_connections_ref.read().as_ref() {
                    render! {
                        IntegrationConnectionsStatus {
                            integration_connections: integration_connections.clone()
                        }
                    }
                }

                div { class: "divider divider-horizontal" }

                match &ui_model_ref.read().loaded_notifications {
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

#[inline_props]
pub fn IntegrationConnectionsStatus(
    cx: Scope,
    integration_connections: Vec<IntegrationConnection>,
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
            IntegrationConnectionStatus {
                connection: integration_connection.clone(),
            }
        }

        div { class: "divider divider-horizontal" }

        for integration_connection in (integration_connections.iter().filter(|c| c.provider.kind().is_task_service() )) {
            IntegrationConnectionStatus {
                connection: integration_connection.clone(),
            }
        }
    }
}

#[inline_props]
pub fn IntegrationConnectionStatus(cx: Scope, connection: IntegrationConnection) -> Element {
    let provider_kind = connection.provider.kind();
    let (connection_style, tooltip) =
        use_memo(cx, &connection.clone(), |connection| match connection {
            IntegrationConnection {
                status: IntegrationConnectionStatus::Validated,
                last_sync_started_at: started_at,
                last_sync_failure_message: None,
                ..
            } => (
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
            ),
            IntegrationConnection {
                status: IntegrationConnectionStatus::Failing,
                failure_message: message,
                ..
            } => (
                "text-error",
                message
                    .map(|message| format!("Connection failed: {message}"))
                    .unwrap_or_else(|| "Connection failed".to_string()),
            ),
            IntegrationConnection {
                last_sync_failure_message: message,
                ..
            } => (
                "text-error",
                message
                    .map(|message| format!("Failed to sync: {message}"))
                    .unwrap_or_else(|| "Failed to sync".to_string()),
            ),
        });

    // tag: New notification integration
    let icon = match provider_kind {
        IntegrationProviderKind::Github => Some(render! {
            Github { class: "w-4 h-4 {connection_style}" }
        }),
        IntegrationProviderKind::Todoist => Some(render! {
            Todoist { class: "w-4 h-4 {connection_style}" }
        }),
        IntegrationProviderKind::Linear => Some(render! {
            Linear { class: "w-4 h-4 {connection_style}" }
        }),
        IntegrationProviderKind::GoogleMail => Some(render! {
            GoogleMail { class: "w-4 h-4 {connection_style}" }
        }),
        _ => None,
    };

    if let Some(icon) = icon {
        return render! {
            div {
                class: "tooltip tooltip-left text-xs",
                "data-tip": "{tooltip}",

                Link {
                    to: Route::SettingsPage {},
                    icon
                }
            }
        };
    }

    render! { div {} }
}
