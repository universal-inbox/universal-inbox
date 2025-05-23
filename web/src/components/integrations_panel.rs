#![allow(non_snake_case)]

use std::collections::HashMap;

use chrono::{Local, SecondsFormat};
use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::{
        bs_icons::{BsBellSlash, BsClockHistory, BsExclamationTriangle, BsPlug, BsTrash},
        md_action_icons::MdCheckCircleOutline,
    },
    Icon,
};
use itertools::Itertools;
use log::warn;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        provider::{IntegrationProvider, IntegrationProviderKind},
        IntegrationConnection, IntegrationConnectionStatus,
    },
    IntegrationProviderStaticConfig,
};

use crate::{
    components::integrations::{
        github::config::GithubProviderConfiguration,
        google_calendar::config::GoogleCalendarProviderConfiguration,
        google_mail::config::GoogleMailProviderConfiguration, icons::IntegrationProviderIcon,
        linear::config::LinearProviderConfiguration, slack::config::SlackProviderConfiguration,
        todoist::config::TodoistProviderConfiguration,
    },
    model::UniversalInboxUIModel,
};

#[component]
pub fn IntegrationsPanel(
    ui_model: Signal<UniversalInboxUIModel>,
    integration_providers: HashMap<IntegrationProviderKind, IntegrationProviderStaticConfig>,
    integration_connections: Vec<IntegrationConnection>,
    on_connect: EventHandler<(IntegrationProviderKind, Option<IntegrationConnection>)>,
    on_disconnect: EventHandler<IntegrationConnection>,
    on_reconnect: EventHandler<IntegrationConnection>,
    on_config_change: EventHandler<(IntegrationConnection, IntegrationConnectionConfig)>,
) -> Element {
    let sorted_integration_providers: Vec<(
        IntegrationProviderKind,
        IntegrationProviderStaticConfig,
    )> = integration_providers
        .into_iter()
        .sorted_by(|(k1, _), (k2, _)| Ord::cmp(&k1.to_string(), &k2.to_string()))
        .collect();

    rsx! {
        div {
            class: "flex flex-col w-auto gap-4 p-8",

            if !integration_connections.iter().any(|c| c.is_connected()) {
                div {
                    class: "alert rounded-md! alert-soft alert-info shadow-lg my-4 text-sm flex gap-2",
                    role: "alert",

                    Icon { class: "min-w-5 h-5", icon: BsPlug }
                    "You have no integrations connected. Connect an integration to get started."
                }
            } else if !integration_connections.iter().any(|c| c.is_connected_task_service()) {
                div {
                    class: "alert rounded-md! alert-soft alert-warning shadow-lg my-4 text-sm flex gap-2",
                    role: "alert",

                    Icon { class: "min-w-5 h-5", icon: BsExclamationTriangle }
                    "To fully use Universal Inbox, you need to connect at least one task management service."
                }
            }

            div {
                class: "flex items-center gap-4 w-full",
                div {
                    class: "leading-none relative shrink-0",
                    span { class: "w-0 h-12 inline-block align-middle" }
                    span { class: "relative text-2xl", "Notifications source services" }
                }
                div { class: "divider grow" }
            }

            for (kind, config) in (sorted_integration_providers.clone()) {
                if kind.is_notification_service() && config.is_enabled {
                    IntegrationSettings {
                        ui_model: ui_model,
                        kind: kind,
                        config: config,
                        connection: integration_connections.iter().find(move |c| c.provider.kind() == kind).cloned(),
                        on_connect: move |c| on_connect.call((kind, c)),
                        on_disconnect: move |c| on_disconnect.call(c),
                        on_reconnect: move |c| on_reconnect.call(c),
                        on_config_change: move |(ic, c)| on_config_change.call((ic, c)),
                    }
                }
            }

            div {
                class: "flex gap-4 w-full",
                div {
                    class: "leading-none relative shrink-0",
                    span { class: "w-0 h-12 inline-block align-middle" }
                    span { class: "relative text-2xl", "Todo list services" }
                }
                div { class: "divider grow" }
            }

            for (kind, config) in (sorted_integration_providers.clone()) {
                if kind.is_task_service() && config.is_enabled {
                    IntegrationSettings {
                        ui_model: ui_model,
                        kind: kind,
                        config: config,
                        connection: integration_connections.iter().find(move |c| c.provider.kind() == kind).cloned(),
                        on_connect: move |c| on_connect.call((kind, c)),
                        on_disconnect: move |c| on_disconnect.call(c),
                        on_reconnect: move |c| on_reconnect.call(c),
                        on_config_change: move |(ic, c)| on_config_change.call((ic, c)),
                    }
                }
            }

            div {
                class: "flex gap-4 w-full",
                div {
                    class: "leading-none relative shrink-0",
                    span { class: "w-0 h-12 inline-block align-middle" }
                    span { class: "relative text-2xl", "Utility services" }
                }
                div { class: "divider grow" }
            }

            for (kind, config) in (sorted_integration_providers.clone()) {
                if !kind.is_notification_service() && !kind.is_task_service() {
                    IntegrationSettings {
                        ui_model: ui_model,
                        kind: kind,
                        config: config,
                        connection: integration_connections.iter().find(move |c| c.provider.kind() == kind).cloned(),
                        on_connect: move |c| on_connect.call((kind, c)),
                        on_disconnect: move |c| on_disconnect.call(c),
                        on_reconnect: move |c| on_reconnect.call(c),
                        on_config_change: move |(ic, c)| on_config_change.call((ic, c)),
                        icon_class: Some("w-10 h-10"),
                    }
                }
            }

        }
    }
}

#[component]
pub fn IntegrationSettings(
    ui_model: Signal<UniversalInboxUIModel>,
    kind: IntegrationProviderKind,
    config: ReadOnlySignal<IntegrationProviderStaticConfig>,
    connection: ReadOnlySignal<Option<Option<IntegrationConnection>>>,
    on_connect: EventHandler<Option<IntegrationConnection>>,
    on_disconnect: EventHandler<IntegrationConnection>,
    on_reconnect: EventHandler<IntegrationConnection>,
    on_config_change: EventHandler<(IntegrationConnection, IntegrationConnectionConfig)>,
    icon_class: Option<&'static str>,
) -> Element {
    let icon_style = icon_class.unwrap_or("w-8 h-8");
    let provider = use_memo(move || {
        if let Some(Some(ic)) = connection() {
            Some(ic.provider.clone())
        } else {
            None
        }
    })();

    let (connection_button_label, connection_button_style, add_disconnect_button) =
        use_memo(move || match connection() {
            Some(Some(
                ic @ IntegrationConnection {
                    status: IntegrationConnectionStatus::Validated,
                    ..
                },
            )) => {
                if ic.has_oauth_scopes(&config().required_oauth_scopes) {
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
                if config().is_enabled {
                    ("Connect", "btn-primary", false)
                } else {
                    ("Not yet implemented", "btn-disabled btn-soft", false)
                }
            }
        })();

    let notifications_sync_message = use_memo(move || match connection() {
        Some(Some(IntegrationConnection {
            status: IntegrationConnectionStatus::Validated,
            provider: IntegrationProvider::Slack { .. },
            provider_user_id: Some(_),
            ..
        })) => Some("🟢 Integration is ready to receive events from Slack".to_string()),
        Some(Some(IntegrationConnection {
            status: IntegrationConnectionStatus::Validated,
            last_notifications_sync_scheduled_at: Some(ref scheduled_at),
            last_notifications_sync_completed_at: Some(_),
            last_notifications_sync_failure_message: None,
            ..
        })) => Some(format!(
            "🟢 Notifications last successfully synced at {}",
            scheduled_at
                .with_timezone(&Local)
                .to_rfc3339_opts(SecondsFormat::Secs, true)
        )),
        Some(Some(IntegrationConnection {
            status: IntegrationConnectionStatus::Validated,
            last_notifications_sync_scheduled_at: Some(ref scheduled_at),
            last_notifications_sync_completed_at: None,
            ..
        })) => Some(format!(
            "🟣 Notifications are currently syncing since {}",
            scheduled_at
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
    })();
    let tasks_sync_message = use_memo(move || match connection() {
        Some(Some(IntegrationConnection {
            status: IntegrationConnectionStatus::Validated,
            last_tasks_sync_scheduled_at: Some(ref scheduled_at),
            last_tasks_sync_completed_at: Some(_),
            last_tasks_sync_failure_message: None,
            ..
        })) => Some(format!(
            "🟢 Tasks last successfully synced at {}",
            scheduled_at
                .with_timezone(&Local)
                .to_rfc3339_opts(SecondsFormat::Secs, true)
        )),
        Some(Some(IntegrationConnection {
            status: IntegrationConnectionStatus::Validated,
            last_tasks_sync_scheduled_at: Some(ref scheduled_at),
            last_tasks_sync_completed_at: None,
            ..
        })) => Some(format!(
            "🟣 Tasks are currently syncing since {}",
            scheduled_at
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
    })();

    let has_all_oauth_scopes = use_memo(move || {
        if let Some(Some(ic)) = connection() {
            let result = ic.has_oauth_scopes(&config().required_oauth_scopes);
            if ic.status == IntegrationConnectionStatus::Validated && !result {
                warn!(
                    "{kind} is connected, but it is missing some permissions: required OAuth scopes: {:?} vs registered OAuth scopes: {:?}", 
                    config().required_oauth_scopes, ic.registered_oauth_scopes
                );
            }
            result
        } else {
            false
        }
    })();

    rsx! {
        div {
            class: "card w-full bg-base-200",

            div {
                class: "card-body text-sm",

                div {
                    class: "flex flex-col sm:flex-row gap-4",

                    div {
                        class: "card-title flex gap-2 items-center justify-center sm:justify-start grow",
                        figure { class: "p-2", IntegrationProviderIcon { class: icon_style, provider_kind: kind } }
                        "{config().name}"
                    }

                    div {
                        class: "flex gap-4",

                        if add_disconnect_button {
                            div {
                                class: "card-actions flex-1 justify-start items-center",

                                button {
                                    class: "btn btn-primary",
                                    onclick: move |_| {
                                        if let Some(Some(c)) = connection() {
                                            on_disconnect.call(c);
                                        }
                                    },

                                    "Disconnect"
                                }
                            }
                        }

                        div {
                            class: "card-actions flex-1 justify-end items-center",

                            button {
                                class: "btn {connection_button_style}",
                                onclick: move |_| {
                                    match connection() {
                                        Some(Some(c @ IntegrationConnection { status: IntegrationConnectionStatus::Validated, .. })) => if has_all_oauth_scopes { on_disconnect.call(c) } else { on_reconnect.call(c) },
                                        Some(Some(c @ IntegrationConnection { status: IntegrationConnectionStatus::Failing, .. })) => on_reconnect.call(c),
                                        Some(Some(c @ IntegrationConnection { status: IntegrationConnectionStatus::Created, .. })) => on_connect.call(Some(c)),
                                        _ => on_connect.call(None),
                                    }
                                },

                                "{connection_button_label}"
                            }
                        }
                    }
                }

                div {
                    class: "flex flex-col grow justify-center items-start",
                    if let Some(notifications_sync_message) = &notifications_sync_message {
                        span { "{notifications_sync_message}" }
                    }
                    if let Some(tasks_sync_message) = &tasks_sync_message {
                        span { "{tasks_sync_message}" }
                    }
                    if notifications_sync_message.is_none() && tasks_sync_message.is_none() {
                        span { }
                    }
                }

                if let Some(Some(IntegrationConnection { failure_message: Some(failure_message), .. })) = connection() {
                    div {
                        class: "alert rounded-md! alert-soft alert-error shadow-lg text-sm flex gap-2",
                        role: "alert",

                        Icon { class: "min-w-5 h-5", icon: BsExclamationTriangle }
                        span { "{failure_message}" }
                    }
                }

                if let Some(Some(IntegrationConnection { status: IntegrationConnectionStatus::Validated, .. })) = connection() {
                    if !has_all_oauth_scopes {
                        div {
                            class: "alert rounded-md! alert-soft alert-warning shadow-lg text-sm flex gap-2",
                            role: "alert",

                            Icon { class: "min-w-5 h-5", icon: BsExclamationTriangle }
                            div {
                                class: "flex flex-col gap-1",
                                span { "{kind} is connected, but it is missing some permissions. Some Universal Inbox features may not work properly." }
                                span { "Please reconnect the {kind} connection to grant the necessary permissions." }
                            }
                        }
                    }
                }

                if let Some(provider) = provider {
                    if let Some(Some(connection)) = connection() {
                        IntegrationConnectionProviderConfiguration {
                            ui_model: ui_model,
                            on_config_change: move |c| on_config_change.call((connection.clone(), c)),
                            provider: provider.clone(),
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn IconForAction(action: String) -> Element {
    let icon = match action.as_str() {
        "delete" => rsx! { Icon { class: "w-5 h-5", icon: BsTrash } },
        "unsubscribe" => rsx! { Icon { class: "w-5 h-5", icon: BsBellSlash } },
        "snooze" => rsx! { Icon { class: "w-5 h-5", icon: BsClockHistory } },
        "complete" => rsx! { Icon { class: "w-5 h-5", icon: MdCheckCircleOutline  } },
        _ => rsx! { div { class: "w-5 h-5" } },
    };

    rsx! {
        button {
            class: "btn btn-soft btn-square pointer-events-none",
            { icon }
        }
    }
}

#[component]
pub fn IntegrationConnectionProviderConfiguration(
    provider: IntegrationProvider,
    ui_model: Signal<UniversalInboxUIModel>,
    on_config_change: EventHandler<IntegrationConnectionConfig>,
) -> Element {
    match provider {
        IntegrationProvider::GoogleCalendar { config } => rsx! {
            GoogleCalendarProviderConfiguration {
                on_config_change: move |c| on_config_change.call(c),
                config: config.clone(),
            }
        },
        IntegrationProvider::GoogleMail { config, context } => rsx! {
            GoogleMailProviderConfiguration {
                on_config_change: move |c| on_config_change.call(c),
                config: config.clone(),
                context: context.clone(),
            }
        },
        IntegrationProvider::Github { config } => rsx! {
            GithubProviderConfiguration {
                on_config_change: move |c| on_config_change.call(c),
                config: config.clone()
            }
        },
        IntegrationProvider::Todoist { config, .. } => rsx! {
            TodoistProviderConfiguration {
                on_config_change: move |c| on_config_change.call(c),
                config: config.clone()
            }
        },
        IntegrationProvider::Linear { config } => rsx! {
            LinearProviderConfiguration {
                ui_model: ui_model,
                on_config_change: move |c| on_config_change.call(c),
                config: config.clone()
            }
        },
        IntegrationProvider::Slack { config, .. } => rsx! {
            SlackProviderConfiguration {
                ui_model: ui_model,
                on_config_change: move |c| on_config_change.call(c),
                config: config.clone(),
            }
        },
        _ => rsx! {},
    }
}
