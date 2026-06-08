#![allow(non_snake_case)]

use std::collections::HashMap;

use chrono::{DateTime, TimeDelta, Utc};
use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use itertools::Itertools;
use log::warn;

use universal_inbox::{
    IntegrationProviderStaticConfig,
    integration_connection::{
        IntegrationConnection, IntegrationConnectionId, IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        provider::{IntegrationProvider, IntegrationProviderKind},
    },
    user::UserPreferencesPatch,
};

use crate::{
    components::{
        ai_agents_card::AiAgentsCard,
        integrations::{
            github::config::GithubProviderConfiguration,
            google_calendar::config::GoogleCalendarProviderConfiguration,
            google_drive::config::GoogleDriveProviderConfiguration,
            google_mail::config::GoogleMailProviderConfiguration,
            linear::config::LinearProviderConfiguration, slack::config::SlackProviderConfiguration,
            ticktick::config::TickTickProviderConfiguration,
            todoist::config::TodoistProviderConfiguration,
        },
        markdown::Markdown,
        task_manager_picker::ProviderIcon,
        ui::{
            BrandTile, BrandTileSize, Button, ButtonVariant, Card, CardBody, CardHeader, CardMeta,
            CardRight, CardVariant, Overline, PageHeader, StatusLeaf, StatusLeafVariant,
            TaskMgrOption, TaskMgrValue, UISelect, UISelectOption,
        },
    },
    model::{LoadState, UniversalInboxUIModel},
    services::{
        integration_connection_service::TASK_SERVICE_INTEGRATION_CONNECTIONS,
        user_preferences_service::{USER_PREFERENCES, UserPreferencesCommand},
    },
    utils::format_elapsed_time,
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

    let user_preferences_service = use_coroutine_handle::<UserPreferencesCommand>();
    let show_default_task_manager = matches!(
        &*TASK_SERVICE_INTEGRATION_CONNECTIONS.read(),
        LoadState::Loaded(connections) if connections.len() >= 2
    );
    let default_task_manager = USER_PREFERENCES
        .read()
        .as_ref()
        .and_then(|p| p.default_task_manager_provider_kind);

    let total_enabled = sorted_integration_providers
        .iter()
        .filter(|(_, c)| c.is_enabled)
        .count();
    let healthy_count = integration_connections
        .iter()
        .filter(|c| {
            c.status == IntegrationConnectionStatus::Validated
                && c.has_oauth_scopes(
                    &sorted_integration_providers
                        .iter()
                        .find(|(k, _)| *k == c.provider.kind())
                        .map(|(_, p)| p.required_oauth_scopes.clone())
                        .unwrap_or_default(),
                )
        })
        .count();
    let last_full_sync = integration_connections
        .iter()
        .filter_map(|c| {
            c.last_notifications_sync_completed_at
                .into_iter()
                .chain(c.last_tasks_sync_completed_at)
                .max()
        })
        .max();

    let failing_connections: Vec<_> = integration_connections
        .iter()
        .filter(|c| {
            c.status == IntegrationConnectionStatus::Failing
                || (c.status == IntegrationConnectionStatus::Validated
                    && !c.has_oauth_scopes(
                        &sorted_integration_providers
                            .iter()
                            .find(|(k, _)| *k == c.provider.kind())
                            .map(|(_, p)| p.required_oauth_scopes.clone())
                            .unwrap_or_default(),
                    ))
        })
        .collect();

    rsx! {
        div {
            PageHeader {
                title: "Settings".to_string(),
                subtitle: Some("Manage which services sync into your inbox.".to_string()),
            }

            if !integration_connections.iter().any(|c| c.is_connected()) {
                div {
                    class: "settings-status-bar status-warn",
                    role: "alert",

                    span { class: "icon-[lucide--plug] size-4" }
                    span { "No integrations connected. Connect an integration to get started." }
                }
            } else if !failing_connections.is_empty() {
                div {
                    class: "settings-status-bar status-warn",
                    role: "alert",

                    span { class: "icon-[lucide--alert-circle] size-4" }
                    {
                        let count = failing_connections.len();
                        let label = if count > 1 { "integrations need" } else { "integration needs" };
                        rsx! { span { strong { "{count} {label} attention" } } }
                    }
                }
            } else if !integration_connections.iter().any(|c| c.is_connected_task_service()) {
                div {
                    class: "settings-status-bar status-info",
                    role: "status",

                    span { class: "icon-[lucide--info] size-4" }
                    span { "Connect a task management service to unlock task syncing and planning features." }
                }
            } else {
                div {
                    class: "settings-status-bar status-ok",
                    role: "status",

                    span { class: "icon-[lucide--check-circle] size-4" }
                    span {
                        strong { "{healthy_count} of {total_enabled} integrations connected and healthy." }
                        if let Some(when) = last_full_sync {
                            " Last full sync "
                            "{format_elapsed_time(when)} ago."
                        }
                    }
                }
            }

            Overline { "Notification sources" }

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

            Overline { class: "mt-4".to_string(), "Todo list services" }

            if show_default_task_manager {
                Card {
                    variant: CardVariant::Integration,
                    CardHeader {
                        interactive: false,
                        class: "max-md:flex-wrap".to_string(),

                        div {
                            class: "flex items-center justify-center shrink-0 size-[26px] bg-transparent border border-ui-border rounded-ui-sm",
                            span { class: "icon-[lucide--zap] size-4" }
                        }

                        CardMeta {
                            name: "Default task manager".to_string(),
                            description: rsx! { "Used for quick actions across notifications" },
                        }

                        CardRight {
                            class: "max-md:basis-full max-md:mt-1".to_string(),
                                UISelect::<IntegrationProviderKind> {
                                    value: use_signal(|| default_task_manager),
                                    options: vec![
                                        UISelectOption::new(IntegrationProviderKind::Todoist, "Todoist"),
                                        UISelectOption::new(IntegrationProviderKind::TickTick, "TickTick"),
                                    ],
                                    on_change: move |provider_kind: Option<IntegrationProviderKind>| {
                                        user_preferences_service.send(
                                            UserPreferencesCommand::Patch(UserPreferencesPatch {
                                                default_task_manager_provider_kind: Some(provider_kind),
                                                ..Default::default()
                                            })
                                        );
                                    },
                                    placeholder: "Choose…".to_string(),
                                    name: "default-task-manager".to_string(),
                                    render_value: use_callback(move |opt: UISelectOption<IntegrationProviderKind>| {
                                        rsx! { TaskMgrValue {
                                            logo: rsx! { ProviderIcon { kind: Some(opt.value) } },
                                            label: opt.label,
                                        } }
                                    }),
                                    render_option: use_callback(move |opt: UISelectOption<IntegrationProviderKind>| {
                                        rsx! { TaskMgrOption {
                                            logo: rsx! { ProviderIcon { kind: Some(opt.value) } },
                                            label: opt.label,
                                        } }
                                    }),
                                    width: "260px".to_string(),
                                }
                            }
                    }
                }
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

            Overline { class: "mt-4".to_string(), "Utility services" }

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
                    }
                }
            }

            // AI agents connect to the MCP server as OAuth2 clients (initiated
            // by the agent), so they aren't IntegrationProviderKind providers —
            // render their card directly. See `ai_agents_card`.
            AiAgentsCard {}

        }
    }
}

#[component]
pub fn IntegrationSettings(
    ui_model: Signal<UniversalInboxUIModel>,
    kind: IntegrationProviderKind,
    config: ReadSignal<IntegrationProviderStaticConfig>,
    connection: ReadSignal<Option<Option<IntegrationConnection>>>,
    on_connect: EventHandler<Option<IntegrationConnection>>,
    on_disconnect: EventHandler<IntegrationConnection>,
    on_reconnect: EventHandler<IntegrationConnection>,
    on_config_change: EventHandler<(IntegrationConnection, IntegrationConnectionConfig)>,
) -> Element {
    // Tick once a minute so relative-time strings refresh while the page is open.
    let mut tick = use_signal(|| 0u32);
    use_future(move || async move {
        loop {
            TimeoutFuture::new(60_000).await;
            tick += 1;
        }
    });

    let provider = use_memo(move || {
        if let Some(Some(ic)) = connection() {
            Some(ic.provider.clone())
        } else {
            None
        }
    })();

    let has_all_oauth_scopes = use_memo(move || {
        if let Some(Some(ic)) = connection() {
            let result = ic.has_oauth_scopes(&config().required_oauth_scopes);
            if ic.status == IntegrationConnectionStatus::Validated && !result {
                warn!(
                    "{kind} is connected, but it is missing some permissions: required OAuth scopes: {:?} vs registered OAuth scopes: {:?}",
                    config().required_oauth_scopes,
                    ic.registered_oauth_scopes
                );
            }
            result
        } else {
            false
        }
    })();

    // (variant, prefix, when) for the sync-row.
    // variant ∈ {"ok", "error", "active", "pending"}; when is a timestamp to format relatively.
    let sync_summary = use_memo(move || {
        let _ = tick();
        match connection() {
            Some(Some(IntegrationConnection {
                status: IntegrationConnectionStatus::Validated,
                provider: IntegrationProvider::Slack { .. },
                provider_user_id: Some(_),
                ..
            })) => Some((
                "ok",
                "Connected — ready to receive events from Slack".to_string(),
                None::<DateTime<Utc>>,
            )),
            Some(Some(IntegrationConnection {
                status: IntegrationConnectionStatus::Validated,
                last_notifications_sync_scheduled_at: Some(scheduled_at),
                last_notifications_sync_completed_at: Some(_),
                last_notifications_sync_failure_message: None,
                ..
            })) => Some((
                "ok",
                format!(
                    "Notifications last synced {} ago",
                    format_elapsed_time(scheduled_at)
                ),
                Some(scheduled_at),
            )),
            Some(Some(IntegrationConnection {
                status: IntegrationConnectionStatus::Validated,
                last_notifications_sync_scheduled_at: Some(scheduled_at),
                last_notifications_sync_completed_at: None,
                ..
            })) => Some((
                "active",
                format!(
                    "Notifications syncing since {} ago",
                    format_elapsed_time(scheduled_at)
                ),
                Some(scheduled_at),
            )),
            Some(Some(IntegrationConnection {
                status: IntegrationConnectionStatus::Validated,
                last_tasks_sync_scheduled_at: Some(scheduled_at),
                last_tasks_sync_completed_at: Some(_),
                last_tasks_sync_failure_message: None,
                ..
            })) => Some((
                "ok",
                format!(
                    "Tasks last synced {} ago",
                    format_elapsed_time(scheduled_at)
                ),
                Some(scheduled_at),
            )),
            Some(Some(IntegrationConnection {
                status: IntegrationConnectionStatus::Validated,
                last_tasks_sync_scheduled_at: Some(scheduled_at),
                last_tasks_sync_completed_at: None,
                ..
            })) => Some((
                "active",
                format!(
                    "Tasks syncing since {} ago",
                    format_elapsed_time(scheduled_at)
                ),
                Some(scheduled_at),
            )),
            Some(Some(IntegrationConnection {
                status: IntegrationConnectionStatus::Validated,
                last_notifications_sync_failure_message: Some(message),
                ..
            })) => Some((
                "error",
                format!("Notifications sync issue: {message}"),
                None,
            )),
            Some(Some(IntegrationConnection {
                status: IntegrationConnectionStatus::Validated,
                last_tasks_sync_failure_message: Some(message),
                ..
            })) => Some(("error", format!("Tasks sync issue: {message}"), None)),
            Some(Some(
                c @ IntegrationConnection {
                    status: IntegrationConnectionStatus::Validated,
                    last_notifications_sync_started_at: None,
                    ..
                },
            )) => {
                if c.provider.is_notification_service() {
                    Some(("pending", "Notifications not yet synced".to_string(), None))
                } else if c.provider.is_task_service() {
                    Some(("pending", "Tasks not yet synced".to_string(), None))
                } else {
                    Some(("ok", "Connected".to_string(), None))
                }
            }
            Some(Some(IntegrationConnection {
                status: IntegrationConnectionStatus::Failing,
                failure_message: Some(message),
                ..
            })) => Some(("error", message, None)),
            Some(Some(IntegrationConnection {
                status: IntegrationConnectionStatus::Failing,
                ..
            })) => Some((
                "error",
                "Integration connection failing — please reconnect".to_string(),
                None,
            )),
            _ => None,
        }
    })();

    let extension_bridge_message = use_memo(move || {
        let Some(Some(ref ic)) = connection() else {
            return None;
        };
        let IntegrationProvider::Slack {
            config: ref slack_config,
            context: Some(ref ctx),
        } = ic.provider
        else {
            return None;
        };
        if !slack_config.message_config.extension_enabled {
            return None;
        }
        let heartbeat_fresh = ctx
            .last_extension_heartbeat_at
            .map(|hb| Utc::now() - hb < TimeDelta::seconds(120))
            .unwrap_or(false);
        if !heartbeat_fresh {
            return Some(
                "Browser extension not polling. Check it is installed and running.".to_string(),
            );
        }
        if ctx.extension_credentials.is_empty() {
            return Some(
                "Browser extension polling but no Slack tab detected. Open app.slack.com or grant the extension permission to access the tab.".to_string(),
            );
        }
        let matching_cred = ctx
            .extension_credentials
            .iter()
            .find(|c| c.team_id == ctx.team_id);
        if matching_cred.is_none() {
            let ext_teams = ctx
                .extension_credentials
                .iter()
                .map(|c| c.team_id.0.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return Some(format!(
                "Browser extension workspace mismatch. Extension sees team {ext_teams}, expected {}.",
                ctx.team_id.0
            ));
        }
        let matching_cred = matching_cred.unwrap();
        if let Some(ref expected_uid) = ic.provider_user_id
            && matching_cred.user_id != *expected_uid
        {
            return Some(format!(
                "Browser extension user mismatch. Extension sees user {}, expected {expected_uid}.",
                matching_cred.user_id
            ));
        }
        None
    })();

    let mut is_expanded = use_signal(|| false);

    let (status_leaf_variant, status_label) = match connection() {
        Some(Some(ref ic)) if ic.status == IntegrationConnectionStatus::Validated => {
            if !has_all_oauth_scopes {
                (StatusLeafVariant::Error, "Needs reconnect")
            } else if ic.is_sync_degraded() {
                (StatusLeafVariant::SyncIssue, "Sync issue")
            } else {
                (StatusLeafVariant::Connected, "Connected")
            }
        }
        Some(Some(ref ic)) if ic.status == IntegrationConnectionStatus::Failing => {
            (StatusLeafVariant::Error, "Error")
        }
        _ => (StatusLeafVariant::Disconnected, "Not connected"),
    };

    let has_connection = matches!(
        connection(),
        Some(Some(IntegrationConnection {
            status: IntegrationConnectionStatus::Validated | IntegrationConnectionStatus::Failing,
            ..
        }))
    );
    let has_error = matches!(
        connection(),
        Some(Some(IntegrationConnection {
            status: IntegrationConnectionStatus::Failing,
            ..
        }))
    ) || (has_connection && !has_all_oauth_scopes);

    let needs_reconnect = match connection() {
        Some(Some(IntegrationConnection {
            status: IntegrationConnectionStatus::Failing,
            ..
        })) => true,
        Some(Some(_)) => has_connection && !has_all_oauth_scopes,
        _ => false,
    };

    // Header meta line (collapsed view): the same string as the sync row, or "Not connected".
    let header_meta: Option<String> = if !has_connection {
        Some("Not connected".to_string())
    } else if let Some(Some(IntegrationConnection {
        failure_message: Some(ref msg),
        ..
    })) = connection()
    {
        Some(msg.clone())
    } else if has_connection && !has_all_oauth_scopes {
        Some("Missing some permissions — please reconnect".to_string())
    } else if let Some((_, ref text, _)) = sync_summary {
        Some(text.clone())
    } else {
        None
    };

    let card_modifier_class = {
        let mut classes = String::new();
        if has_error {
            classes.push_str("has-error");
        }
        if !has_connection {
            if !classes.is_empty() {
                classes.push(' ');
            }
            classes.push_str("disconnected-card");
        }
        classes
    };
    let card_expanded = has_connection && is_expanded();

    let failure_message = match connection() {
        Some(Some(IntegrationConnection {
            failure_message: Some(msg),
            ..
        })) => Some(msg),
        _ => None,
    };

    rsx! {
        Card {
            variant: CardVariant::Integration,
            expanded: card_expanded,
            class: card_modifier_class,

            if has_connection {
                // Click-target header — mirrors the utility composition emitted
                // by `<CardHeader>` but renders a `role="button"` div with
                // keyboard handlers so the entire row is the affordance.
                // The `group` token lets the chevron react to hover via
                // `group-hover:bg-ui-base-200`.
                div {
                    class: "group flex items-center gap-2.5 px-3.5 py-3 cursor-pointer \
                            select-none transition-colors duration-[var(--ui-dur-fast)] \
                            hover:bg-ui-surface-hover focus-visible:outline-2 \
                            focus-visible:outline-ui-primary focus-visible:-outline-offset-2 \
                            focus-visible:rounded-ui-lg",
                    role: "button",
                    tabindex: 0,
                    aria_expanded: "{is_expanded}",
                    aria_label: "Toggle {config().name} settings",
                    onclick: move |_| is_expanded.toggle(),
                    onkeydown: move |event: KeyboardEvent| {
                        if event.key() == Key::Enter || event.key() == Key::Character(" ".to_string()) {
                            event.prevent_default();
                            is_expanded.toggle();
                        }
                    },

                    BrandTile { provider: kind, size: BrandTileSize::Md }

                    CardMeta {
                        name: config().name.clone(),
                        description: header_meta.as_ref().map(|desc| rsx! { "{desc}" }),
                        hide_description: is_expanded(),
                    }

                    CardRight {
                        StatusLeaf {
                            variant: status_leaf_variant,
                            label: status_label.to_string(),
                        }
                        // Chevron disc — rotates 180° when expanded and tints
                        // base-200 on header hover. The `transition-transform`
                        // keeps the rotation smooth.
                        span {
                            class: if is_expanded() {
                                "size-6 inline-flex items-center justify-center rounded-full \
                                 text-ui-base-muted shrink-0 transition-transform duration-150 \
                                 rotate-180 group-hover:bg-ui-base-200"
                            } else {
                                "size-6 inline-flex items-center justify-center rounded-full \
                                 text-ui-base-muted shrink-0 transition-transform duration-150 \
                                 group-hover:bg-ui-base-200"
                            },
                            span { class: "icon-[lucide--chevron-down] size-4" }
                        }
                    }
                }

                CardBody {
                    expandable: true,

                    div {
                        class: "connections-list",

                        if let Some((variant, ref text, _when)) = sync_summary {
                            // Sync row — small pill with a glowing LED, status
                            // text, and the disconnect/reconnect action. The
                            // `.sync-led*` class hooks are retained because
                            // they bind to `color-mix()` glow rules in CSS.
                            div {
                                class: if variant == "error" {
                                    "flex items-center gap-2 px-3 py-2 mb-1 \
                                     bg-ui-base-200 border border-ui-border-light \
                                     rounded-ui-sm text-[length:var(--ui-text-sm)] text-ui-error \
                                     [&_b]:font-semibold [&_b]:text-ui-error"
                                } else {
                                    "flex items-center gap-2 px-3 py-2 mb-1 \
                                     bg-ui-base-200 border border-ui-border-light \
                                     rounded-ui-sm text-[length:var(--ui-text-sm)] text-ui-base-muted \
                                     [&_b]:font-semibold [&_b]:text-ui-base-content"
                                },

                                span { class: "sync-led {variant}" }
                                span { class: "flex-1 min-w-0", "{text}" }
                                div {
                                    class: "ml-auto inline-flex gap-1.5 shrink-0",

                                    if needs_reconnect {
                                        Button {
                                            variant: ButtonVariant::Warning,
                                            icon_class: "icon-[lucide--refresh-cw]".to_string(),
                                            onclick: move |_| {
                                                match connection() {
                                                    Some(Some(c @ IntegrationConnection { status: IntegrationConnectionStatus::Failing, .. })) => on_reconnect.call(c),
                                                    Some(Some(c)) if !has_all_oauth_scopes => on_reconnect.call(c),
                                                    _ => {}
                                                }
                                            },
                                            "Reconnect"
                                        }
                                    } else {
                                        Button {
                                            variant: ButtonVariant::Danger,
                                            icon_class: "icon-[lucide--unplug]".to_string(),
                                            onclick: move |_| {
                                                if let Some(Some(c)) = connection() {
                                                    on_disconnect.call(c);
                                                }
                                            },
                                            "Disconnect"
                                        }
                                    }
                                }
                            }
                        }

                        if let Some(ref msg) = extension_bridge_message {
                            div { class: "pod-failure", "{msg}" }
                        }

                        if let Some(ref msg) = failure_message {
                            if sync_summary.map(|(v, _, _)| v) != Some("error") {
                                div { class: "pod-failure", "{msg}" }
                            }
                        }

                        if let Some(ref provider) = provider {
                            if let Some(Some(ref conn)) = connection() {
                                div {
                                    class: "pod-config",
                                    IntegrationConnectionProviderConfiguration {
                                        ui_model: ui_model,
                                        on_config_change: {
                                            let conn = conn.clone();
                                            move |c| on_config_change.call((conn.clone(), c))
                                        },
                                        provider: provider.clone(),
                                        provider_user_id: conn.provider_user_id.clone(),
                                        connection_id: conn.id,
                                    }
                                }
                            }
                        }

                        if let Some(ref warning_message) = config().warning_message {
                            if !warning_message.is_empty() {
                                div {
                                    class: "integration-alert warning",
                                    role: "alert",
                                    span { class: "icon-[lucide--triangle-alert] h-5 min-w-5" }
                                    Markdown { text: "{warning_message}" }
                                }
                            }
                        }
                    }
                }
            } else {
                CardHeader {
                    interactive: false,
                    BrandTile { provider: kind, size: BrandTileSize::Md }

                    CardMeta {
                        name: config().name.clone(),
                        description: rsx! { "Not connected" },
                        muted_name: true,
                    }

                    CardRight {
                        if config().is_enabled {
                            Button {
                                variant: ButtonVariant::Primary,
                                icon_class: "icon-[lucide--plug]".to_string(),
                                onclick: move |_| {
                                    match connection() {
                                        Some(Some(c)) => on_connect.call(Some(c)),
                                        _ => on_connect.call(None),
                                    }
                                },
                                "Connect"
                            }
                        } else {
                            StatusLeaf {
                                variant: StatusLeafVariant::Disconnected,
                                label: "Not yet implemented".to_string(),
                            }
                        }
                    }
                }

                if let Some(ref warning_message) = config().warning_message {
                    if !warning_message.is_empty() {
                        div {
                            class: "card-body-expandable",
                            style: "display: block;",
                            div {
                                class: "connections-list",
                                div {
                                    class: "integration-alert warning",
                                    role: "alert",
                                    span { class: "icon-[lucide--triangle-alert] h-5 min-w-5" }
                                    Markdown { text: "{warning_message}" }
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
pub fn IntegrationConnectionProviderConfiguration(
    provider: IntegrationProvider,
    provider_user_id: ReadSignal<Option<Option<String>>>,
    connection_id: ReadSignal<IntegrationConnectionId>,
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
        IntegrationProvider::GoogleDrive { config, .. } => rsx! {
            GoogleDriveProviderConfiguration {
                on_config_change: move |c| on_config_change.call(c),
                config: config.clone(),
            }
        },
        IntegrationProvider::Github { config } => rsx! {
            GithubProviderConfiguration {
                on_config_change: move |c| on_config_change.call(c),
                config: config.clone()
            }
        },
        IntegrationProvider::TickTick { config, .. } => rsx! {
            TickTickProviderConfiguration {
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
        IntegrationProvider::Slack { config, context } => rsx! {
            SlackProviderConfiguration {
                ui_model: ui_model,
                on_config_change: move |c| on_config_change.call(c),
                config: config.clone(),
                context: context.clone(),
                provider_user_id,
                connection_id,
            }
        },
        _ => rsx! {},
    }
}
