#![allow(non_snake_case)]

use std::collections::HashMap;

use chrono::{Local, SecondsFormat};
use dioxus::prelude::*;

use universal_inbox::{
    IntegrationProviderStaticConfig,
    integration_connection::{
        IntegrationConnection, IntegrationConnectionStatus as ConnectionStatus,
        provider::{IntegrationProvider, IntegrationProviderKind},
    },
};

use crate::{
    components::{
        flyonui::tooltip::{Tooltip, TooltipPlacement},
        integrations::icons::IntegrationProviderIcon,
        ui::KeyboardHint,
    },
    config::APP_CONFIG,
    route::Route,
    services::integration_connection_service::INTEGRATION_CONNECTIONS,
};

pub fn Footer() -> Element {
    let (message, message_class) = use_memo(move || {
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
                "error",
            );
        };
        let has_degraded_sync = integration_connections.iter().any(|c| c.is_sync_degraded());
        if has_degraded_sync {
            return (
                Some("Some integrations are experiencing sync issues. Retrying automatically."),
                "warning",
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
            return (
                Some("Some integrations are missing permissions, please reconnect them."),
                "warning",
            );
        }
        let has_slack_extension_enabled = integration_connections.iter().any(|c| {
            matches!(
                &c.provider,
                IntegrationProvider::Slack {
                    config,
                    ..
                } if config.message_config.extension_enabled
            )
        });
        if has_slack_extension_enabled {
            // Check if the extension has a recent heartbeat via context
            let extension_not_detected = integration_connections.iter().any(|c| {
                matches!(
                    &c.provider,
                    IntegrationProvider::Slack {
                        context: Some(ctx),
                        config,
                        ..
                    } if config.message_config.extension_enabled
                        && ctx.last_extension_heartbeat_at
                            .map(|hb| (chrono::Utc::now() - hb).num_seconds() > 120)
                            .unwrap_or(true)
                )
            });
            if extension_not_detected {
                return (
                    Some("Slack browser extension not detected. Install or check it is running."),
                    "warning",
                );
            }
        }
        (None, "")
    })();

    rsx! {
        footer {
            class: "h-7 shrink-0 flex items-center justify-between px-3 bg-ui-surface border-t border-ui-border text-xs text-ui-base-muted z-50",

            if let Some(integration_connections) = INTEGRATION_CONNECTIONS().as_ref() {
                if let Some(app_config) = APP_CONFIG().as_ref() {
                    IntegrationConnectionsStatus {
                        integration_connections: integration_connections.clone(),
                        integration_providers: app_config.integration_providers.clone()
                    }
                }
            }

            if let Some(message) = message {
                Link {
                    to: Route::SettingsPage {},
                    class: "max-md:hidden",
                    div { class: "footer-alert {message_class}",
                        span { "{message}" }
                    }
                }
            }

            div { class: "flex items-center gap-2.5 max-md:hidden",
                KeyboardHint { keys: vec!["↑↓".to_string()], label: "navigate".to_string() }
                KeyboardHint { keys: vec!["d".to_string()], label: "delete".to_string() }
                KeyboardHint { keys: vec!["s".to_string()], label: "snooze".to_string() }
                KeyboardHint { keys: vec!["t".to_string()], label: "task".to_string() }
                KeyboardHint { keys: vec!["?".to_string()], label: "help".to_string() }
            }
        }
    }
}

#[component]
pub fn IntegrationConnectionsStatus(
    integration_connections: Vec<IntegrationConnection>,
    integration_providers: HashMap<IntegrationProviderKind, IntegrationProviderStaticConfig>,
) -> Element {
    let collect_group = |predicate: fn(IntegrationProviderKind) -> bool| {
        let mut group: Vec<(IntegrationConnection, IntegrationProviderStaticConfig)> =
            integration_connections
                .iter()
                .filter(|c| predicate(c.provider.kind()))
                .filter_map(|c| {
                    integration_providers
                        .get(&c.provider.kind())
                        .filter(|cfg| cfg.is_enabled)
                        .map(|cfg| (c.clone(), cfg.clone()))
                })
                .collect();
        group.sort_by(|(a, _), (b, _)| {
            a.provider
                .kind()
                .to_string()
                .cmp(&b.provider.kind().to_string())
        });
        group
    };

    let notification_group = collect_group(|k| k.is_notification_service());
    let task_group = collect_group(|k| k.is_task_service());
    let utility_group = collect_group(|k| !k.is_notification_service() && !k.is_task_service());

    let need_divider_after_notifications =
        !notification_group.is_empty() && (!task_group.is_empty() || !utility_group.is_empty());
    let need_divider_after_tasks = !task_group.is_empty() && !utility_group.is_empty();

    rsx! {
        div { class: "flex items-center gap-1.5",
            for (connection, config) in notification_group {
                IntegrationConnectionStatus { connection, config }
            }
            if need_divider_after_notifications {
                span {
                    class: "inline-block w-px h-3.5 bg-ui-border mx-1 shrink-0",
                    aria_hidden: "true"
                }
            }
            for (connection, config) in task_group {
                IntegrationConnectionStatus { connection, config }
            }
            if need_divider_after_tasks {
                span {
                    class: "inline-block w-px h-3.5 bg-ui-border mx-1 shrink-0",
                    aria_hidden: "true"
                }
            }
            for (connection, config) in utility_group {
                IntegrationConnectionStatus { connection, config }
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
    // Returns (legacy class hook, dot-color utility, optional opacity utility, tooltip).
    // The legacy `.footer-integration.syncing` class is load-bearing: it is the
    // selector that triggers `@keyframes footer-led-pulse` on the child
    // `.footer-status-dot` (see `web/css/universal-inbox.css`). All other
    // status modifiers (`connected`, `error`, `disconnected`) are kept as
    // semantic hooks and to preserve `data-*`-style debuggability, but their
    // visual effect (dot fill color, opacity) is now driven by the per-status
    // utility classes below.
    let (status_class, dot_color_class, dim_class, tooltip) = use_memo(move || match &connection {
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
                let (status, dot_color) = if connection_is_syncing {
                    ("syncing", "bg-ui-warning")
                } else {
                    ("connected", "bg-ui-success")
                };
                (
                    status,
                    dot_color,
                    "",
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
                    "error",
                    "bg-ui-error",
                    "",
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
            "error",
            "bg-ui-error",
            "",
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
            "error",
            "bg-ui-error",
            "",
            format!("{provider_kind} sync is degraded: {message}"),
        ),
        IntegrationConnection { .. } => (
            "disconnected",
            "bg-ui-base-muted",
            "opacity-[0.55]",
            format!("{provider_kind} connection is not connected."),
        ),
    })();

    rsx! {
        Link {
            to: Route::SettingsPage {},
            div {
                // `.footer-integration` + `.syncing` are load-bearing class hooks
                // for the `footer-led-pulse` keyframe (see
                // `web/css/universal-inbox.css`). Do not remove. Everything else
                // is utility-driven.
                class: "footer-integration {status_class} flex items-center relative px-1.5 py-0.5 rounded-ui-sm transition-colors duration-150 hover:bg-ui-surface-hover cursor-default {dim_class}",
                Tooltip {
                    text: tooltip,
                    placement: TooltipPlacement::Top,
                    span {
                        class: "relative inline-flex items-center justify-center w-4 h-4",
                        IntegrationProviderIcon { class: "w-4 h-4".to_string(), provider_kind }
                        // `.footer-status-dot` is the load-bearing target of the
                        // pulse keyframe under `.footer-integration.syncing`. The
                        // dot's fill color is utility-driven via `{dot_color_class}`.
                        span {
                            class: "footer-status-dot absolute -bottom-[3px] -right-[4px] w-[7px] h-[7px] rounded-full border-[1.5px] border-ui-surface shrink-0 {dot_color_class}"
                        }
                    }
                }
            }
        }
    }
}
