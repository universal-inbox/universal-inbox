use chrono::{Local, SecondsFormat};
use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsPlug, Icon};
use dioxus_router::Link;
use fermi::use_atom_ref;

use universal_inbox::integration_connection::{
    IntegrationConnection, IntegrationConnectionStatus, IntegrationProviderKind,
};

use crate::{
    components::icons::{github, linear, todoist},
    model::UI_MODEL,
    services::{
        integration_connection_service::INTEGRATION_CONNECTIONS,
        notification_service::NotificationCommand,
    },
};

pub fn footer(cx: Scope) -> Element {
    let integration_connections_ref = use_atom_ref(cx, INTEGRATION_CONNECTIONS);
    let ui_model_ref = use_atom_ref(cx, UI_MODEL);
    let notification_service = use_coroutine_handle::<NotificationCommand>(cx).unwrap();

    cx.render(rsx! {
        footer {
            class: "w-full",

            hr {}
            div {
                class: "w-full flex gap-2 p-2 justify-end items-center",

                if let Some(integration_connections) = integration_connections_ref.read().as_ref() {
                    rsx!(integration_connections_status { integration_connections: integration_connections.clone() })
                }

                match &ui_model_ref.read().loaded_notifications {
                    Some(Ok(count)) => rsx!(div {
                        class: "tooltip tooltip-left",
                        "data-tip": "{count} notifications loaded",
                        button {
                            class: "badge badge-success text-xs",
                            onclick: |_| notification_service.send(NotificationCommand::Refresh),
                            "{count}"
                        }
                    }),
                    Some(Err(error)) => rsx!(div {
                        class: "tooltip tooltip-left tooltip-error",
                        "data-tip": "{error}",
                        button {
                            class: "badge badge-error text-xs",
                            onclick: |_| notification_service.send(NotificationCommand::Refresh),
                            "0"
                        }
                    }),
                    None => rsx!(span { class: "loading loading-ring loading-xs" }),
                }
            }
        }
    })
}

#[inline_props]
pub fn integration_connections_status(
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

    cx.render(rsx!(
        if let Some(tooltip) = connection_issue_tooltip {
            rsx!(
                div {
                    class: "tooltip tooltip-left text-xs text-error",
                    "data-tip": "{tooltip}",

                    Link {
                        to: "/settings",
                        title: "Sync status",
                        Icon { class: "w-5 h-5" icon: BsPlug }
                    }
                }
            )
        }

        for integration_connection in (&*integration_connections) {
            integration_connection_status {
                connection: integration_connection.clone(),
            }
        }
    ))
}

#[inline_props]
pub fn integration_connection_status(cx: Scope, connection: IntegrationConnection) -> Element {
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
                            "{} successfully synced at {}",
                            connection.provider_kind,
                            started_at
                                .with_timezone(&Local)
                                .to_rfc3339_opts(SecondsFormat::Secs, true)
                        )
                    })
                    .unwrap_or_else(|| format!("{} successfully synced", connection.provider_kind)),
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

    let icon = match connection.provider_kind {
        IntegrationProviderKind::Github => rsx!(self::github {
            class: "w-4 h-4 {connection_style}"
        }),
        IntegrationProviderKind::Todoist => rsx!(self::todoist {
            class: "w-4 h-4 {connection_style}"
        }),
        IntegrationProviderKind::Linear => rsx!(self::linear {
            class: "w-4 h-4 {connection_style}"
        }),
    };

    cx.render(rsx! {
        div {
            class: "tooltip tooltip-left text-xs",
            "data-tip": "{tooltip}",

            Link {
                to: "/settings",
                title: "Sync status",
                icon
            }
        }
    })
}
