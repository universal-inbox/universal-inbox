use std::collections::HashMap;

use chrono::{Local, SecondsFormat};
use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{
        BsBellSlash, BsCheck2, BsClockHistory, BsExclamationTriangle, BsInfoCircle, BsPlug,
        BsSlack, BsTrash,
    },
    Icon,
};
use itertools::Itertools;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionStatus, IntegrationProviderKind,
    },
    IntegrationProviderConfig,
};

use crate::components::icons::{
    github, google_mail, googledocs, linear, notion, ticktick, todoist,
};

#[inline_props]
pub fn integrations_panel<'a>(
    cx: Scope,
    integration_providers: HashMap<IntegrationProviderKind, IntegrationProviderConfig>,
    integration_connections: Vec<IntegrationConnection>,
    on_connect: EventHandler<'a, (IntegrationProviderKind, Option<&'a IntegrationConnection>)>,
    on_disconnect: EventHandler<'a, &'a IntegrationConnection>,
    on_reconnect: EventHandler<'a, &'a IntegrationConnection>,
) -> Element {
    let sorted_integration_providers: Vec<(&IntegrationProviderKind, &IntegrationProviderConfig)> =
        integration_providers
            .iter()
            .sorted_by(|(k1, _), (k2, _)| Ord::cmp(&k1.to_string(), &k2.to_string()))
            .collect();

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
                    span { class: "relative text-2xl", "Notifications source services" }
                }
                div { class: "divider grow" }
            }

            for (kind, config) in (&sorted_integration_providers) {
                if kind.is_notification_service() && config.is_implemented {
                    rsx!(
                        integration_settings {
                            kind: **kind,
                            config: (*config).clone(),
                            connection: integration_connections.iter().find(|c| c.provider_kind == **kind).cloned(),
                            on_connect: |c| on_connect.call((**kind, c)),
                            on_disconnect: |c| on_disconnect.call(c),
                            on_reconnect: |c| on_reconnect.call(c),
                        }
                    )
                }
            }

            for (kind, config) in (&sorted_integration_providers) {
                if kind.is_notification_service() && !config.is_implemented {
                    rsx!(
                        integration_settings {
                            kind: **kind,
                            config: (*config).clone(),
                            connection: integration_connections.iter().find(|c| c.provider_kind == **kind).cloned(),
                            on_connect: |c| on_connect.call((**kind, c)),
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
                    span { class: "relative text-2xl", "Todo list services" }
                }
                div { class: "divider grow" }
            }

            for (kind, config) in (&sorted_integration_providers) {
                if kind.is_task_service() && config.is_implemented {
                    rsx!(
                        integration_settings {
                            kind: **kind,
                            config: (*config).clone(),
                            connection: integration_connections.iter().find(|c| c.provider_kind == **kind).cloned(),
                            on_connect: |c| on_connect.call((**kind, c)),
                            on_disconnect: |c| on_disconnect.call(c),
                            on_reconnect: |c| on_reconnect.call(c),
                        }
                    )
                }
            }

            for (kind, config) in (&sorted_integration_providers) {
                if kind.is_task_service() && !config.is_implemented {
                    rsx!(
                        integration_settings {
                            kind: **kind,
                            config: (*config).clone(),
                            connection: integration_connections.iter().find(|c| c.provider_kind == **kind).cloned(),
                            on_connect: |c| on_connect.call((**kind, c)),
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
    // tag: New notification integration
    let icon = match kind {
        IntegrationProviderKind::Github => rsx!(self::github { class: "w-8 h-8" }),
        IntegrationProviderKind::Linear => rsx!(self::linear { class: "w-8 h-8" }),
        IntegrationProviderKind::GoogleMail => rsx!(self::google_mail { class: "w-8 h-8" }),
        IntegrationProviderKind::Notion => rsx!(self::notion { class: "w-8 h-8" }),
        IntegrationProviderKind::GoogleDocs => rsx!(self::googledocs { class: "w-8 h-8" }),
        IntegrationProviderKind::Slack => rsx!(Icon {
            class: "w-8 h-8",
            icon: BsSlack
        }),
        IntegrationProviderKind::Todoist => rsx!(self::todoist { class: "w-8 h-8" }),
        IntegrationProviderKind::TickTick => rsx!(self::ticktick { class: "w-8 h-8" }),
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
                last_sync_started_at: None,
                ..
            })) => (
                "Disconnect",
                "btn-success",
                Some("🟠 Not yet synced".to_string()),
                if kind.is_notification_service() {
                    Some("Synchronize notifications")
                } else if kind.is_task_service() {
                    Some("Synchronize tasks from the inbox as notifications")
                } else {
                    None
                },
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

                self::documentation { config: config.clone() }
            }
        }
    ))
}

#[inline_props]
pub fn documentation(cx: Scope, config: IntegrationProviderConfig) -> Element {
    let mut doc_for_actions: Vec<(&String, &String)> = config.doc_for_actions.iter().collect();
    doc_for_actions.sort_by(|e1, e2| e1.0.cmp(e2.0));

    cx.render(rsx!(
        if let Some(ref warning_message) = config.warning_message {
            rsx!(
                div {
                    class: "alert alert-warning shadow-lg my-4 py-2",
                    Icon { class: "w-5 h-5" icon: BsExclamationTriangle }
                    p { class: "max-w-full prose prose-sm", dangerous_inner_html: "{warning_message}" }
                }
            )
        }

        if !config.doc.is_empty() {
            rsx!(
                details {
                    class: "collapse collapse-arrow bg-neutral-focus",
                    summary {
                        class: "collapse-title text-lg font-medium min-h-min",
                        div {
                            class: "flex gap-2 items-center",
                            Icon { class: "w-5 h-5 text-info", icon: BsInfoCircle }
                            "Documentation"
                        }
                    }

                    div {
                        class: "collapse-content flex flex-col gap-2",

                        p { class: "py-2", "{config.doc}"}

                        div { class: "text-base", "Actions on notifications" }
                        if !doc_for_actions.is_empty() {
                            rsx!(
                                table {
                                    class: "table-auto",

                                    tbody {
                                        for (action, doc) in doc_for_actions.iter() {
                                            rsx!(
                                                tr {
                                                    td { icon_for_action { action: action.to_string() } }
                                                    td { class: "pr-4 font-semibold", "{action}" }
                                                    td { "{doc}" }
                                                }
                                            )
                                        }
                                    }
                                }
                            )
                        }
                    }
                }
            )
        }
    ))
}

#[inline_props]
pub fn icon_for_action(cx: Scope, action: String) -> Element {
    let icon = match action.as_str() {
        "delete" => rsx!(Icon {
            class: "w-5 h-5",
            icon: BsTrash
        }),
        "unsubscribe" => rsx!(Icon {
            class: "w-5 h-5",
            icon: BsBellSlash
        }),
        "snooze" => rsx!(Icon {
            class: "w-5 h-5",
            icon: BsClockHistory
        }),
        "complete" => rsx!(Icon {
            class: "w-5 h-5",
            icon: BsCheck2
        }),
        _ => rsx!(div { class: "w-5 h-5" }),
    };

    cx.render(rsx!(button {
        class: "btn btn-ghost btn-square pointer-events-none",
        icon
    }))
}
