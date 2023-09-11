use std::collections::HashMap;

use chrono::{Local, SecondsFormat};
use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{BsExclamationTriangle, BsPlug},
    Icon,
};

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionStatus, IntegrationProviderKind,
    },
    IntegrationProviderConfig,
};

use crate::components::icons::{github, linear, notion, todoist};

#[inline_props]
pub fn integrations_panel<'a>(
    cx: Scope,
    integration_providers: HashMap<IntegrationProviderKind, IntegrationProviderConfig>,
    integration_connections: Vec<IntegrationConnection>,
    on_connect: EventHandler<'a, (IntegrationProviderKind, Option<&'a IntegrationConnection>)>,
    on_disconnect: EventHandler<'a, &'a IntegrationConnection>,
    on_reconnect: EventHandler<'a, &'a IntegrationConnection>,
) -> Element {
    cx.render(rsx!(
        div {
            class: "flex flex-col w-auto gap-4 p-8",

            if !integration_connections.iter().any(|c| c.is_connected()) {
                rsx!(
                    div {
                        class: "alert alert-info shadow-lg my-4",

                        Icon { class: "w-5 h-5" icon: BsPlug }
                        "You have no integrations connected. Connect an integration to get started."
                    }
                )
            } else if !integration_connections.iter().any(|c| c.is_connected_task_service()) {
                rsx!(
                    div {
                        class: "alert alert-warning shadow-lg my-4",

                        Icon { class: "w-5 h-5" icon: BsExclamationTriangle }
                        "To fully use Universal Inbox, you need to connect at least one task management service."
                    }
                )
            }

            div {
                class: "flex gap-4 w-full",
                div {
                    class: "leading-none relative",
                    span { class: "w-0 h-12 inline-block align-middle" }
                    span { class: "relative text-2xl text-gray-600", "Notifications source services" }
                }
                div { class: "divider grow" }
            }

            for (kind, config) in (&*integration_providers) {
                if kind.is_notification_service() && config.is_implemented {
                    rsx!(
                        integration_settings {
                            kind: *kind,
                            config: config.clone(),
                            connection: integration_connections.iter().find(|c| c.provider_kind == *kind).cloned(),
                            on_connect: |c| on_connect.call((*kind, c)),
                            on_disconnect: |c| on_disconnect.call(c),
                            on_reconnect: |c| on_reconnect.call(c),
                        }
                    )
                }
            }

            for (kind, config) in (&*integration_providers) {
                if kind.is_notification_service() && !config.is_implemented {
                    rsx!(
                        integration_settings {
                            kind: *kind,
                            config: config.clone(),
                            connection: integration_connections.iter().find(|c| c.provider_kind == *kind).cloned(),
                            on_connect: |c| on_connect.call((*kind, c)),
                            on_disconnect: |c| on_disconnect.call(c),
                            on_reconnect: |c| on_reconnect.call(c),
                        }
                    )
                }
            }

            div {
                class: "flex gap-4 w-full",
                div {
                    class: "leading-none relative",
                    span { class: "w-0 h-12 inline-block align-middle" }
                    span { class: "relative text-2xl text-gray-600", "Todo list services" }
                }
                div { class: "divider grow" }
            }

            for (kind, config) in (&*integration_providers) {
                if kind.is_task_service() && config.is_implemented {
                    rsx!(
                        integration_settings {
                            kind: *kind,
                            config: config.clone(),
                            connection: integration_connections.iter().find(|c| c.provider_kind == *kind).cloned(),
                            on_connect: |c| on_connect.call((*kind, c)),
                            on_disconnect: |c| on_disconnect.call(c),
                            on_reconnect: |c| on_reconnect.call(c),
                        }
                    )
                }
            }

            for (kind, config) in (&*integration_providers) {
                if kind.is_task_service() && !config.is_implemented {
                    rsx!(
                        integration_settings {
                            kind: *kind,
                            config: config.clone(),
                            connection: integration_connections.iter().find(|c| c.provider_kind == *kind).cloned(),
                            on_connect: |c| on_connect.call((*kind, c)),
                            on_disconnect: |c| on_disconnect.call(c),
                            on_reconnect: |c| on_reconnect.call(c),
                        }
                    )
                }
            }
        }
    ))
}

#[inline_props]
pub fn integration_settings<'a>(
    cx: Scope,
    kind: IntegrationProviderKind,
    config: IntegrationProviderConfig,
    connection: Option<Option<IntegrationConnection>>,
    on_connect: EventHandler<'a, Option<&'a IntegrationConnection>>,
    on_disconnect: EventHandler<'a, &'a IntegrationConnection>,
    on_reconnect: EventHandler<'a, &'a IntegrationConnection>,
) -> Element {
    let icon = match kind {
        IntegrationProviderKind::Github => rsx!(self::github { class: "w-8 h-8" }),
        IntegrationProviderKind::Linear => rsx!(self::linear { class: "w-8 h-8" }),
        IntegrationProviderKind::Notion => rsx!(self::notion { class: "w-8 h-8" }),
        IntegrationProviderKind::Todoist => rsx!(self::todoist { class: "w-8 h-8" }),
    };

    let (connection_button_label, connection_button_style, sync_message, feature_label) =
        use_memo(cx, &connection.clone(), |connection| match connection {
            Some(Some(IntegrationConnection {
                status: IntegrationConnectionStatus::Validated,
                last_sync_started_at: Some(ref started_at),
                last_sync_failure_message: None,
                ..
            })) => (
                "Disconnect",
                "btn-success",
                Some(format!(
                    "🟢 Last successfully synced at {}",
                    started_at
                        .with_timezone(&Local)
                        .to_rfc3339_opts(SecondsFormat::Secs, true)
                )),
                if kind.is_notification_service() {
                    Some("Synchronize notifications")
                } else if kind.is_task_service() {
                    Some("Synchronize tasks from the inbox as notifications")
                } else {
                    None
                },
            ),
            Some(Some(IntegrationConnection {
                status: IntegrationConnectionStatus::Validated,
                last_sync_failure_message: Some(message),
                ..
            })) => (
                "Disconnect",
                "btn-success",
                Some(format!("🔴 Last sync failed: {message}")),
                None,
            ),
            Some(Some(IntegrationConnection {
                status: IntegrationConnectionStatus::Validated,
                last_sync_started_at: None,
                ..
            })) => (
                "Disconnect",
                "btn-success",
                Some("🟠 Not yet synced".to_string()),
                None,
            ),
            Some(Some(IntegrationConnection {
                status: IntegrationConnectionStatus::Failing,
                ..
            })) => ("Reconnect", "btn-error", None, None),
            _ => {
                if config.is_implemented {
                    ("Connect", "btn-primary", None, None)
                } else {
                    ("Not yet implemented", "btn-disabled btn-ghost", None, None)
                }
            }
        });

    cx.render(rsx!(
        div {
            class: "card w-full bg-neutral text-neutral-content",

            div {
                class: "card-body",

                div {
                    class: "flex flex-row gap-4",

                    div {
                        class: "card-title",
                        figure { class: "p-2", icon }
                        "{config.name}"
                    }
                    div {
                        class: "flex grow justify-start items-center",
                        if let Some(sync_message) = sync_message {
                            rsx!(span { "{sync_message}" })
                        } else {
                            rsx!(span {})
                        }
                    }
                    div {
                        class: "card-actions justify-end",

                        button {
                            class: "btn {connection_button_style}",
                            onclick: move |_| {
                                match connection {
                                    Some(Some(c @ IntegrationConnection { status: IntegrationConnectionStatus::Validated, .. })) => on_disconnect.call(c),
                                    Some(Some(c @ IntegrationConnection { status: IntegrationConnectionStatus::Failing, .. })) => on_reconnect.call(c),
                                    Some(Some(c @ IntegrationConnection { status: IntegrationConnectionStatus::Created, .. })) => on_connect.call(Some(c)),
                                    _ => on_connect.call(None),
                                }
                            },

                            "{connection_button_label}"
                        }
                    }
                }

                if let Some(Some(IntegrationConnection { failure_message: Some(failure_message), .. })) = connection {
                    rsx!(
                        div {
                            class: "alert alert-error shadow-lg",

                            Icon { class: "w-5 h-5" icon: BsExclamationTriangle }
                            span { "{failure_message}" }
                        }
                    )
                }

                if let Some(feature_label) = feature_label {
                    rsx!(
                        div {
                            class: "form-control",
                            label {
                                class: "cursor-pointer label",
                                span { class: "label-text, text-neutral-content", "{feature_label}" }
                                input { r#type: "checkbox", class: "toggle toggle-primary", disabled: true, checked: true }
                            }
                        }
                    )
                }

                if let Some(comment) = &config.comment {
                    rsx!(
                        div {
                            class: "alert alert-warning shadow-lg",

                            span { "{comment}" }
                        }
                    )
                }
            }
        }
    ))
}
