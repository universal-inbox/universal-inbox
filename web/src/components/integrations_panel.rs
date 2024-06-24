#![allow(non_snake_case)]

use std::collections::HashMap;

use chrono::{Local, SecondsFormat};
use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{
        BsBellSlash, BsCheck2, BsClockHistory, BsExclamationTriangle, BsInfoCircle, BsPlug, BsTrash,
    },
    Icon,
};
use fermi::UseAtomRef;
use itertools::Itertools;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        provider::{IntegrationProvider, IntegrationProviderKind},
        IntegrationConnection, IntegrationConnectionStatus,
    },
    IntegrationProviderStaticConfig,
};

use crate::{
    components::{
        integrations::{
            github::config::GithubProviderConfiguration,
            google_mail::config::GoogleMailProviderConfiguration, icons::IntegrationProviderIcon,
            linear::config::LinearProviderConfiguration, slack::config::SlackProviderConfiguration,
            todoist::config::TodoistProviderConfiguration,
        },
        markdown::Markdown,
    },
    model::UniversalInboxUIModel,
};

#[component]
pub fn IntegrationsPanel<'a>(
    cx: Scope,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
    integration_providers: HashMap<IntegrationProviderKind, IntegrationProviderStaticConfig>,
    integration_connections: Vec<IntegrationConnection>,
    on_connect: EventHandler<'a, (IntegrationProviderKind, Option<&'a IntegrationConnection>)>,
    on_disconnect: EventHandler<'a, &'a IntegrationConnection>,
    on_reconnect: EventHandler<'a, &'a IntegrationConnection>,
    on_config_change: EventHandler<'a, (&'a IntegrationConnection, IntegrationConnectionConfig)>,
) -> Element {
    let sorted_integration_providers: Vec<(
        &IntegrationProviderKind,
        &IntegrationProviderStaticConfig,
    )> = integration_providers
        .iter()
        .sorted_by(|(k1, _), (k2, _)| Ord::cmp(&k1.to_string(), &k2.to_string()))
        .collect();

    render! {
        div {
            class: "flex flex-col w-auto gap-4 p-8",

            if !integration_connections.iter().any(|c| c.is_connected()) {
                render! {
                    div {
                        class: "alert alert-info shadow-lg my-4",

                        Icon { class: "w-5 h-5", icon: BsPlug }
                        "You have no integrations connected. Connect an integration to get started."
                    }
                }
            } else if !integration_connections.iter().any(|c| c.is_connected_task_service()) {
                render! {
                    div {
                        class: "alert alert-warning shadow-lg my-4",

                        Icon { class: "w-5 h-5", icon: BsExclamationTriangle }
                        "To fully use Universal Inbox, you need to connect at least one task management service."
                    }
                }
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
                    render! {
                        IntegrationSettings {
                            ui_model_ref: ui_model_ref.clone(),
                            kind: **kind,
                            config: (*config).clone(),
                            connection: integration_connections.iter().find(|c| c.provider.kind() == **kind).cloned(),
                            on_connect: |c| on_connect.call((**kind, c)),
                            on_disconnect: |c| on_disconnect.call(c),
                            on_reconnect: |c| on_reconnect.call(c),
                            on_config_change: |(ic, c)| on_config_change.call((ic, c)),
                        }
                    }
                }
            }

            for (kind, config) in (&sorted_integration_providers) {
                if kind.is_notification_service() && !config.is_implemented {
                    render! {
                        IntegrationSettings {
                            ui_model_ref: ui_model_ref.clone(),
                            kind: **kind,
                            config: (*config).clone(),
                            connection: integration_connections.iter().find(|c| c.provider.kind() == **kind).cloned(),
                            on_connect: |c| on_connect.call((**kind, c)),
                            on_disconnect: |c| on_disconnect.call(c),
                            on_reconnect: |c| on_reconnect.call(c),
                            on_config_change: |(ic, c)| on_config_change.call((ic, c)),
                        }
                    }
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
                    render! {
                        IntegrationSettings {
                            ui_model_ref: ui_model_ref.clone(),
                            kind: **kind,
                            config: (*config).clone(),
                            connection: integration_connections.iter().find(|c| c.provider.kind() == **kind).cloned(),
                            on_connect: |c| on_connect.call((**kind, c)),
                            on_disconnect: |c| on_disconnect.call(c),
                            on_reconnect: |c| on_reconnect.call(c),
                            on_config_change: |(ic, c)| on_config_change.call((ic, c)),
                        }
                    }
                }
            }

            for (kind, config) in (&sorted_integration_providers) {
                if kind.is_task_service() && !config.is_implemented {
                    render! {
                        IntegrationSettings {
                            ui_model_ref: ui_model_ref.clone(),
                            kind: **kind,
                            config: (*config).clone(),
                            connection: integration_connections.iter().find(|c| c.provider.kind() == **kind).cloned(),
                            on_connect: |c| on_connect.call((**kind, c)),
                            on_disconnect: |c| on_disconnect.call(c),
                            on_reconnect: |c| on_reconnect.call(c),
                            on_config_change: |(ic, c)| on_config_change.call((ic, c)),
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn IntegrationSettings<'a>(
    cx: Scope,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
    kind: IntegrationProviderKind,
    config: IntegrationProviderStaticConfig,
    connection: Option<Option<IntegrationConnection>>,
    on_connect: EventHandler<'a, Option<&'a IntegrationConnection>>,
    on_disconnect: EventHandler<'a, &'a IntegrationConnection>,
    on_reconnect: EventHandler<'a, &'a IntegrationConnection>,
    on_config_change: EventHandler<'a, (&'a IntegrationConnection, IntegrationConnectionConfig)>,
) -> Element {
    let provider = use_memo(cx, &connection.clone(), |connection| {
        if let Some(Some(ic)) = &connection {
            Some(ic.provider.clone())
        } else {
            None
        }
    });

    let (connection_button_label, connection_button_style, add_disconnect_button) =
        use_memo(cx, &connection.clone(), |connection| match connection {
            Some(Some(
                ic @ IntegrationConnection {
                    status: IntegrationConnectionStatus::Validated,
                    ..
                },
            )) => {
                if ic.has_oauth_scopes(&config.required_oauth_scopes) {
                    ("Disconnect", "btn-success", false)
                } else {
                    ("Reconnect", "btn-warning", true)
                }
            }
            Some(Some(IntegrationConnection {
                status: IntegrationConnectionStatus::Failing,
                ..
            })) => ("Reconnect", "btn-error", true),
            _ => {
                if config.is_implemented {
                    ("Connect", "btn-primary", false)
                } else {
                    ("Not yet implemented", "btn-disabled btn-ghost", false)
                }
            }
        });

    let notifications_sync_message =
        use_memo(cx, &connection.clone(), |connection| match connection {
            Some(Some(IntegrationConnection {
                status: IntegrationConnectionStatus::Validated,
                provider: IntegrationProvider::Slack { .. },
                provider_user_id: Some(_),
                ..
            })) => Some("🟢 Integration is ready to receive events from Slack".to_string()),
            Some(Some(IntegrationConnection {
                status: IntegrationConnectionStatus::Validated,
                last_notifications_sync_started_at: Some(ref started_at),
                last_notifications_sync_completed_at: Some(_),
                last_notifications_sync_failure_message: None,
                ..
            })) => Some(format!(
                "🟢 Notifications last successfully synced at {}",
                started_at
                    .with_timezone(&Local)
                    .to_rfc3339_opts(SecondsFormat::Secs, true)
            )),
            Some(Some(IntegrationConnection {
                status: IntegrationConnectionStatus::Validated,
                last_notifications_sync_started_at: Some(ref started_at),
                last_notifications_sync_completed_at: None,
                ..
            })) => Some(format!(
                "🟣 Notifications are currently syncing since {}",
                started_at
                    .with_timezone(&Local)
                    .to_rfc3339_opts(SecondsFormat::Secs, true)
            )),
            Some(Some(IntegrationConnection {
                status: IntegrationConnectionStatus::Validated,
                last_notifications_sync_failure_message: Some(message),
                ..
            })) => Some(format!("🔴 Notifications last sync failed: {message}")),
            Some(Some(
                c @ IntegrationConnection {
                    status: IntegrationConnectionStatus::Validated,
                    last_notifications_sync_started_at: None,
                    ..
                },
            )) => {
                if c.provider.is_notification_service() {
                    Some("🟠 Notifications Not yet synced".to_string())
                } else {
                    None
                }
            }
            _ => None,
        });
    let tasks_sync_message = use_memo(cx, &connection.clone(), |connection| match connection {
        Some(Some(IntegrationConnection {
            status: IntegrationConnectionStatus::Validated,
            last_tasks_sync_started_at: Some(ref started_at),
            last_tasks_sync_completed_at: Some(_),
            last_tasks_sync_failure_message: None,
            ..
        })) => Some(format!(
            "🟢 Tasks last successfully synced at {}",
            started_at
                .with_timezone(&Local)
                .to_rfc3339_opts(SecondsFormat::Secs, true)
        )),
        Some(Some(IntegrationConnection {
            status: IntegrationConnectionStatus::Validated,
            last_tasks_sync_started_at: Some(ref started_at),
            last_tasks_sync_completed_at: None,
            ..
        })) => Some(format!(
            "🟣 Tasks are currently syncing since {}",
            started_at
                .with_timezone(&Local)
                .to_rfc3339_opts(SecondsFormat::Secs, true)
        )),
        Some(Some(IntegrationConnection {
            status: IntegrationConnectionStatus::Validated,
            last_tasks_sync_failure_message: Some(message),
            ..
        })) => Some(format!("🔴 Tasks last sync failed: {message}")),
        Some(Some(
            c @ IntegrationConnection {
                status: IntegrationConnectionStatus::Validated,
                last_tasks_sync_started_at: None,
                ..
            },
        )) => {
            if c.provider.is_task_service() {
                Some("🟠 Tasks not yet synced".to_string())
            } else {
                None
            }
        }
        _ => None,
    });

    let has_all_oauth_scopes = use_memo(cx, &connection.clone(), |connection| {
        if let Some(Some(ic)) = connection {
            ic.has_oauth_scopes(&config.required_oauth_scopes)
        } else {
            false
        }
    });

    render! {
        div {
            class: "card w-full bg-base-200 text-base-content",

            div {
                class: "card-body",

                div {
                    class: "flex flex-row gap-4",

                    div {
                        class: "card-title",
                        figure { class: "p-2", IntegrationProviderIcon { class: "w-8 h-8", provider_kind: *kind } }
                        "{config.name}"
                    }
                    div {
                        class: "flex flex-col grow justify-center items-start",
                        if let Some(notifications_sync_message) = notifications_sync_message {
                            render! { span { "{notifications_sync_message}" } }
                        }
                        if let Some(tasks_sync_message) = tasks_sync_message {
                            render! { span { "{tasks_sync_message}" } }
                        }
                        if notifications_sync_message.is_none() && tasks_sync_message.is_none() {
                            render! { span { } }
                        }
                    }
                    div {
                        class: "card-actions justify-end",

                        button {
                            class: "btn {connection_button_style}",
                            onclick: move |_| {
                                match connection {
                                    Some(Some(c @ IntegrationConnection { status: IntegrationConnectionStatus::Validated, .. })) => if *has_all_oauth_scopes { on_disconnect.call(c) } else { on_reconnect.call(c) },
                                    Some(Some(c @ IntegrationConnection { status: IntegrationConnectionStatus::Failing, .. })) => on_reconnect.call(c),
                                    Some(Some(c @ IntegrationConnection { status: IntegrationConnectionStatus::Created, .. })) => on_connect.call(Some(c)),
                                    _ => on_connect.call(None),
                                }
                            },

                            "{connection_button_label}"
                        }
                    }

                    if *add_disconnect_button {
                        render! {
                            div {
                                class: "card-actions justify-end",

                                button {
                                    class: "btn btn-primary",
                                    onclick: move |_| {
                                        if let Some(Some(c)) = connection {
                                            on_disconnect.call(c);
                                        }
                                    },

                                    "Disconnect"
                                }
                            }
                        }
                    }
                }

                if let Some(Some(IntegrationConnection { failure_message: Some(failure_message), .. })) = connection {
                    render! {
                        div {
                            class: "alert alert-error shadow-lg",

                            Icon { class: "w-5 h-5", icon: BsExclamationTriangle }
                            span { "{failure_message}" }
                        }
                    }
                }

                if let Some(Some(IntegrationConnection { status: IntegrationConnectionStatus::Validated, .. })) = connection {
                    if !*has_all_oauth_scopes {
                        render! {
                            div {
                                class: "alert alert-warning shadow-lg",

                                Icon { class: "w-5 h-5", icon: BsExclamationTriangle }
                                div {
                                    class: "flex flex-col gap-1",
                                    span { "{kind} is connected, but it is missing some permissions. Some Universal Inbox features may not work properly." }
                                    span { "Please reconnect the {kind} connection to grant the necessary permissions." }
                                }
                            }
                        }
                    } else {
                        None
                    }
                }

                if let Some(provider) = provider {
                    if let Some(Some(connection)) = connection {
                        render! {
                            IntegrationConnectionProviderConfiguration {
                                ui_model_ref: ui_model_ref.clone(),
                                on_config_change: move |config| on_config_change.call((connection, config)),
                                provider: provider.clone(),
                            }
                        }
                    } else {
                        None
                    }
                }

                Documentation { config: config.clone() }
            }
        }
    }
}

#[component]
pub fn Documentation(cx: Scope, config: IntegrationProviderStaticConfig) -> Element {
    let mut doc_for_actions: Vec<(&String, &String)> = config.doc_for_actions.iter().collect();
    doc_for_actions.sort_by(|e1, e2| e1.0.cmp(e2.0));

    render! {
        if let Some(ref warning_message) = config.warning_message {
            if !warning_message.is_empty() {
                render! {
                    div {
                        class: "alert alert-warning shadow-lg my-4 py-2",
                        Icon { class: "w-5 h-5", icon: BsExclamationTriangle }
                        p { class: "max-w-full prose prose-sm", dangerous_inner_html: "{warning_message}" }
                    }
                }
            } else {
                None
            }
        }

        if !config.doc.is_empty() {
            render! {
                details {
                    class: "collapse collapse-arrow bg-neutral text-neutral-content",
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

                        Markdown { class: "!prose-invert", text: config.doc.clone() }

                        div { class: "text-base", "Actions on notifications" }
                        if !doc_for_actions.is_empty() {
                            render! {
                                table {
                                    class: "table-auto",

                                    tbody {
                                        for (action, doc) in doc_for_actions.iter() {
                                            render! {
                                                tr {
                                                    td { IconForAction { action: action.to_string() } }
                                                    td { class: "pr-4 font-semibold", "{action}" }
                                                    td { "{doc}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn IconForAction(cx: Scope, action: String) -> Element {
    let icon = match action.as_str() {
        "delete" => render! { Icon { class: "w-5 h-5", icon: BsTrash } },
        "unsubscribe" => render! { Icon { class: "w-5 h-5", icon: BsBellSlash } },
        "snooze" => render! { Icon { class: "w-5 h-5", icon: BsClockHistory } },
        "complete" => render! { Icon { class: "w-5 h-5", icon: BsCheck2 } },
        _ => render! { div { class: "w-5 h-5" } },
    };

    render! {
        button {
            class: "btn btn-ghost btn-square pointer-events-none",
            icon
        }
    }
}

#[component]
pub fn IntegrationConnectionProviderConfiguration<'a>(
    cx: Scope,
    provider: IntegrationProvider,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
    on_config_change: EventHandler<'a, IntegrationConnectionConfig>,
) -> Element {
    match provider {
        IntegrationProvider::GoogleMail { config, context } => render! {
            GoogleMailProviderConfiguration {
                on_config_change: |c| on_config_change.call(c),
                config: config.clone(),
                context: context.clone(),
            }
        },
        IntegrationProvider::Github { config } => render! {
            GithubProviderConfiguration {
                on_config_change: |c| on_config_change.call(c),
                config: config.clone()
            }
        },
        IntegrationProvider::Todoist { config, .. } => render! {
            TodoistProviderConfiguration {
                on_config_change: |c| on_config_change.call(c),
                config: config.clone()
            }
        },
        IntegrationProvider::Linear { config } => render! {
            LinearProviderConfiguration {
                ui_model_ref: ui_model_ref.clone(),
                on_config_change: |c| on_config_change.call(c),
                config: config.clone()
            }
        },
        IntegrationProvider::Slack { config } => render! {
            SlackProviderConfiguration {
                ui_model_ref: ui_model_ref.clone(),
                on_config_change: |c| on_config_change.call(c),
                config: config.clone(),
            }
        },
        _ => None,
    }
}
