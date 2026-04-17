#![allow(non_snake_case)]

use dioxus::prelude::dioxus_core::use_drop;
use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use dioxus_free_icons::{
    Icon,
    icons::bs_icons::{BsChatText, BsPuzzle},
};
use log::error;
use reqwest::Method;
use serde_json::json;
use slack_morphism::SlackReactionName;

use chrono::{Local, SecondsFormat};

use universal_inbox::{
    integration_connection::{
        IntegrationConnectionId,
        config::IntegrationConnectionConfig,
        integrations::slack::{
            SlackConfig, SlackContext, SlackEmojiSuggestion, SlackMessageConfig,
            SlackReactionConfig, SlackSyncTaskConfig, SlackSyncType,
        },
    },
    slack_bridge::SlackBridgeStatus,
    task::{PresetDueDate, ProjectSummary, TaskPriority},
    utils::emoji::replace_emoji_code_with_emoji,
};

use crate::{
    components::{
        floating_label_inputs::{FloatingLabelInputSearchSelect, FloatingLabelSelect},
        flyonui::tooltip::{Tooltip, TooltipPlacement},
    },
    config::get_api_base_url,
    model::UniversalInboxUIModel,
    services::{
        api::call_api,
        flyonui::{forget_flyonui_tabs_element, init_flyonui_tabs_element},
    },
};

#[component]
pub fn SlackProviderConfiguration(
    config: ReadSignal<SlackConfig>,
    context: ReadSignal<Option<Option<SlackContext>>>,
    provider_user_id: ReadSignal<Option<Option<String>>>,
    connection_id: ReadSignal<IntegrationConnectionId>,
    ui_model: Signal<UniversalInboxUIModel>,
    on_config_change: EventHandler<IntegrationConnectionConfig>,
) -> Element {
    let emoji = replace_emoji_code_with_emoji(config().reaction_config.reaction_name.0.as_str())
        .unwrap_or("👀".to_string());
    let mut mounted_element: Signal<Option<web_sys::Element>> = use_signal(|| None);

    use_drop(move || {
        if let Some(element) = mounted_element() {
            forget_flyonui_tabs_element(&element);
        }
    });

    rsx! {
        div {
            class: "flex flex-col",

            nav {
                role: "tablist",
                class: "tabs tabs-bordered tabs-sm flex-wrap",
                onmounted: move |element| {
                    let web_element = element.as_web_event();
                    init_flyonui_tabs_element(&web_element);
                    mounted_element.set(Some(web_element));
                },

                button {
                    class: "tab active-tab:tab-active active flex items-center gap-2 rounded-b-none",
                    "type": "button",
                    role: "tab",
                    "data-tab": "#slack-config-tab-reaction",
                    span { "{emoji}" }
                    span { "Reaction" }
                }

                button {
                    class: "tab active-tab:tab-active flex items-center gap-2 rounded-b-none",
                    "type": "button",
                    role: "tab",
                    "data-tab": "#slack-config-tab-mention",
                    Icon { class: "h-5 w-5 min-w-5", icon: BsChatText },
                    span { "Mention" }
                }

                button {
                    class: "tab active-tab:tab-active flex items-center gap-2 rounded-b-none",
                    "type": "button",
                    role: "tab",
                    "data-tab": "#slack-config-tab-extension",
                    Icon { class: "h-5 w-5 min-w-5", icon: BsPuzzle },
                    span { "Extension" }
                }
            }

            div {
                div {
                    id: "slack-config-tab-reaction",
                    role: "tabpanel",
                    class: "bg-base-100 border-base-300 p-6 rounded-b-md flex flex-col gap-2",
                    SlackReactionConfiguration { config, connection_id, ui_model, on_config_change }
                }

                div {
                    id: "slack-config-tab-mention",
                    role: "tabpanel",
                    class: "bg-base-100 border-base-300 p-6 rounded-b-md flex flex-col gap-2 hidden",
                    SlackMessageConfiguration { config, ui_model, on_config_change }
                }

                div {
                    id: "slack-config-tab-extension",
                    role: "tabpanel",
                    class: "bg-base-100 border-base-300 p-6 rounded-b-md flex flex-col gap-2 hidden",
                    SlackExtensionConfiguration { config, context, provider_user_id, on_config_change }
                }
            }
        }
    }
}

#[component]
fn SlackReactionConfiguration(
    config: ReadSignal<SlackConfig>,
    connection_id: ReadSignal<IntegrationConnectionId>,
    ui_model: Signal<UniversalInboxUIModel>,
    on_config_change: EventHandler<IntegrationConnectionConfig>,
) -> Element {
    let mut default_emoji = use_signal(|| "eyes".to_string());
    let mut default_completion_emoji: Signal<Option<String>> = use_signal(|| None);
    let mut default_priority = use_signal(|| Some(TaskPriority::P4));
    let mut default_due_at: Signal<Option<PresetDueDate>> = use_signal(|| None);
    let mut default_project: Signal<Option<ProjectSummary>> = use_signal(|| None);
    let mut task_config_enabled = use_signal(|| false);
    use_memo(move || {
        *default_emoji.write() = config().reaction_config.reaction_name.0.clone();
        *default_completion_emoji.write() = config()
            .reaction_config
            .completion_reaction_name
            .as_ref()
            .map(|name| name.0.clone());
        if let SlackSyncType::AsTasks(config) = config().reaction_config.sync_type {
            *default_priority.write() = Some(config.default_priority);
            default_due_at.write().clone_from(&config.default_due_at);
            *default_project.write() = config.target_project;
            *task_config_enabled.write() = ui_model.read().is_task_actions_enabled;
        } else {
            *task_config_enabled.write() = false;
        }
    });
    let collapse_style = use_memo(move || {
        if task_config_enabled() {
            ""
        } else {
            "hidden overflow-hidden"
        }
    });
    let completion_reaction_collapse_style = use_memo(move || {
        if config().reaction_config.completion_reaction_name.is_some() {
            ""
        } else {
            "hidden overflow-hidden"
        }
    });
    let api_base_url = get_api_base_url().unwrap();
    let as_tasks_disabled =
        !config().reaction_config.sync_enabled || !ui_model.read().is_task_actions_enabled;

    rsx! {
        div {
            class: "flex items-center gap-2",

            label {
                class: "label-text cursor-pointer grow text-sm text-base-content",
                "Synchronize Slack reacted items"
            }
            div {
                class: "relative inline-block",
                input {
                    r#type: "checkbox",
                    class: "switch switch-primary switch-outline peer",
                    oninput: move |event| {
                        on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                            reaction_config: SlackReactionConfig {
                                sync_enabled: event.value() == "true",
                                ..config().reaction_config
                            },
                            ..config()
                        }))
                    },
                    checked: config().reaction_config.sync_enabled
                }
                span {
                    class: "icon-[tabler--check] text-primary absolute start-1 top-1 hidden size-4 peer-checked:block"
                }
                span {
                    class: "icon-[tabler--x] text-neutral absolute end-1 top-1 block size-4 peer-checked:hidden"
                }
            }
        }

        div {
            class: "flex items-center gap-2",
            label {
                class: "label-text cursor-pointer grow text-sm text-base-content",
                "Emoji reaction to synchronize"
            }
            FloatingLabelInputSearchSelect::<SlackEmojiSuggestion> {
                name: "reaction-name-input".to_string(),
                class: "max-w-xs",
                data_select: json!({
                    "value": default_emoji(),
                    "apiUrl": format!("{api_base_url}integration-connections/{}/slack/emojis/search", connection_id()),
                    "apiSearchQueryKey": "matches",
                    "apiFieldsMap": {
                        "id": "name",
                        "val": "name",
                        "title": "display_name"
                    }
                }),
                on_select: move |emoji: Option<SlackEmojiSuggestion>| {
                    on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                        reaction_config: SlackReactionConfig {
                            reaction_name: SlackReactionName(
                                emoji.map(|e| e.name).unwrap_or("eyes".to_string())
                            ),
                            ..config().reaction_config
                        },
                        ..config()
                    }));
                },
            }
        }

        div {
            class: "flex flex-col gap-2 overflow-visible",

            div {
                class: "flex items-center gap-2",
                label {
                    class: "label-text cursor-pointer grow text-sm text-base-content",
                    "Set an emoji reaction when the task is completed"
                }
                div {
                    class: "relative inline-block",
                    input {
                        r#type: "checkbox",
                        class: "switch switch-primary switch-outline peer",
                        disabled: !config().reaction_config.sync_enabled,
                        oninput: move |event| {
                            let completion_reaction_name = if event.value() == "true" {
                                Some(SlackReactionName("white_check_mark".to_string()))
                            } else {
                                None
                            };
                            on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                                reaction_config: SlackReactionConfig {
                                    completion_reaction_name,
                                    ..config().reaction_config
                                },
                                ..config()
                            }))
                        },
                        checked: config().reaction_config.completion_reaction_name.is_some()
                    }
                    span {
                        class: "icon-[tabler--check] text-primary absolute start-1 top-1 hidden size-4 peer-checked:block"
                    }
                    span {
                        class: "icon-[tabler--x] text-neutral absolute end-1 top-1 block size-4 peer-checked:hidden"
                    }
                }
            }

            div {
                class: "collapse transition-[height] duration-300 {completion_reaction_collapse_style} pb-0 pr-0 flex flex-col gap-2",

                div {
                    class: "flex items-center gap-2",
                    label {
                        class: "label-text cursor-pointer grow text-sm text-base-content",
                        "Emoji reaction to set on completion"
                    }
                    FloatingLabelInputSearchSelect::<SlackEmojiSuggestion> {
                        name: "completion-reaction-name-input".to_string(),
                        class: "max-w-xs",
                        disabled: !config().reaction_config.sync_enabled,
                        data_select: json!({
                            "value": default_completion_emoji().unwrap_or_default(),
                            "apiUrl": format!("{api_base_url}integration-connections/{}/slack/emojis/search", connection_id()),
                            "apiSearchQueryKey": "matches",
                            "apiFieldsMap": {
                                "id": "name",
                                "val": "name",
                                "title": "display_name"
                            }
                        }),
                        on_select: move |emoji: Option<SlackEmojiSuggestion>| {
                            on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                                reaction_config: SlackReactionConfig {
                                    completion_reaction_name: emoji.map(|e| SlackReactionName(e.name)),
                                    ..config().reaction_config
                                },
                                ..config()
                            }));
                        },
                    }
                }
            }
        }

        div {
            class: "flex items-center gap-2",
            label {
                class: "label-text cursor-pointer grow text-sm text-base-content",
                "for": "slack-reaction-as-notifications",
                "Synchronize Slack reacted items as notifications"
            }
            input {
                id: "slack-reaction-as-notifications",
                disabled: !config().reaction_config.sync_enabled,
                r#type: "radio",
                class: "radio radio-soft radio-sm",
                name: "reaction-sync-type",
                oninput: move |_event| {
                    on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                        reaction_config: SlackReactionConfig {
                            sync_type: SlackSyncType::AsNotifications,
                            ..config().reaction_config
                        },
                        ..config()
                    }))
                },
                checked: config().reaction_config.sync_type == SlackSyncType::AsNotifications
            }
        }

        div {
            class: "flex flex-col gap-2 overflow-visible",

            Tooltip {
                placement: TooltipPlacement::Bottom,
                disabled: !as_tasks_disabled,
                tooltip_class: "tooltip-error",
                text: "A task management service must be connected to enable this feature",

                div {
                    class: "flex items-center gap-2",
                    label {
                        class: "label-text cursor-pointer grow text-sm text-base-content text-start",
                        "for": "slack-reaction-as-tasks",
                        "Synchronize Slack reacted items as tasks"
                    }
                    input {
                        id: "slack-reaction-as-tasks",
                        disabled: as_tasks_disabled,
                        name: "reaction-sync-type",
                        class: "radio radio-soft radio-sm",
                        r#type: "radio",
                        oninput: move |_event| {
                            on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                                reaction_config: SlackReactionConfig {
                                    sync_type: SlackSyncType::AsTasks(match &config().reaction_config.sync_type {
                                        SlackSyncType::AsTasks(config) => config.clone(),
                                        _ => Default::default(),
                                    }),
                                    ..config().reaction_config
                                },
                                ..config()
                            }))
                        },
                        checked: !(config().reaction_config.sync_type == SlackSyncType::AsNotifications)
                    }
                }
            }

            div {
                class: "collapse transition-[height] duration-300 {collapse_style} pb-0 pr-0 flex flex-col gap-2",

                div {
                    class: "flex items-center gap-2",
                    label {
                        class: "label-text cursor-pointer grow text-sm text-base-content",
                        "Project to assign synchronized tasks to"
                    }
                    FloatingLabelInputSearchSelect::<ProjectSummary> {
                        name: "reaction-project-search-input".to_string(),
                        class: "w-full max-w-xs bg-base-100 rounded-sm",
                        required: true,
                        disabled: !ui_model.read().is_task_actions_enabled,
                        data_select: json!({
                            "value": default_project().map(|p| p.source_id.to_string()),
                            "apiUrl": format!("{api_base_url}tasks/projects/search"),
                            "apiSearchQueryKey": "matches",
                            "apiFieldsMap": {
                                "id": "source_id",
                                "val": "source_id",
                                "title": "name"
                            }
                        }),
                        on_select: move |project: Option<ProjectSummary>| {
                            on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                                reaction_config: SlackReactionConfig {
                                    sync_type: SlackSyncType::AsTasks(match &config().reaction_config.sync_type {
                                        SlackSyncType::AsTasks(config) => SlackSyncTaskConfig {
                                            target_project: project.clone(),
                                            ..config.clone()
                                        },
                                        _ => Default::default(),
                                    }),
                                    ..config().reaction_config
                                },
                                ..config()
                            }))
                        }
                    }
                }

                div {
                    class: "flex items-center gap-2",
                    label {
                        class: "label-text cursor-pointer grow text-sm text-base-content",
                        "Due date to assign to synchronized tasks"
                    }
                    FloatingLabelSelect::<PresetDueDate> {
                        label: None,
                        class: "max-w-xs",
                        name: "task-due-at-input".to_string(),
                        disabled: !ui_model.read().is_task_actions_enabled,
                        default_value: default_due_at().map(|due| due.to_string()).unwrap_or_default(),
                        on_select: move |default_due_at| {
                            on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                                reaction_config: SlackReactionConfig {
                                    sync_type: SlackSyncType::AsTasks(match &config().reaction_config.sync_type {
                                        SlackSyncType::AsTasks(task_config) => SlackSyncTaskConfig {
                                            default_due_at,
                                            ..task_config.clone()
                                        },
                                        _ => SlackSyncTaskConfig {
                                            default_due_at,
                                            ..Default::default()
                                        }
                                    }),
                                    ..config().reaction_config
                                },
                                ..config()
                            }));
                        },

                        option { selected: default_due_at() == Some(PresetDueDate::Today), "{PresetDueDate::Today}" }
                        option { selected: default_due_at() == Some(PresetDueDate::Tomorrow), "{PresetDueDate::Tomorrow}" }
                        option { selected: default_due_at() == Some(PresetDueDate::ThisWeekend), "{PresetDueDate::ThisWeekend}" }
                        option { selected: default_due_at() == Some(PresetDueDate::NextWeek), "{PresetDueDate::NextWeek}" }
                    }
                }

                div {
                    class: "flex items-center gap-2",
                    label {
                        class: "label-text cursor-pointer grow text-sm text-base-content",
                        "Priority to assign to synchronized tasks"
                    }
                    FloatingLabelSelect::<TaskPriority> {
                        label: None,
                        class: "max-w-xs",
                        name: "task-priority-input".to_string(),
                        disabled: !ui_model.read().is_task_actions_enabled,
                        required: true,
                        default_value: "{default_priority().unwrap_or_default()}",
                        on_select: move |priority: Option<TaskPriority>| {
                            on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                                reaction_config: SlackReactionConfig {
                                    sync_type: SlackSyncType::AsTasks(match &config().reaction_config.sync_type {
                                        SlackSyncType::AsTasks(task_config) => SlackSyncTaskConfig {
                                            default_priority: priority.unwrap_or_default(),
                                            ..task_config.clone()
                                        },
                                        _ => SlackSyncTaskConfig {
                                            default_priority: priority.unwrap_or_default(),
                                            ..Default::default()
                                        },
                                    }),
                                    ..config().reaction_config
                                },
                                ..config()
                            }));
                        },

                        option { selected: default_priority() == Some(TaskPriority::P1), value: "1", "🔴 Priority 1" }
                        option { selected: default_priority() == Some(TaskPriority::P2), value: "2", "🟠 Priority 2" }
                        option { selected: default_priority() == Some(TaskPriority::P3), value: "3", "🟡 Priority 3" }
                        option { selected: default_priority() == Some(TaskPriority::P4), value: "4", "🔵 Priority 4" }
                    }
                }
            }
        }
    }
}

#[component]
fn SlackMessageConfiguration(
    config: ReadSignal<SlackConfig>,
    ui_model: Signal<UniversalInboxUIModel>,
    on_config_change: EventHandler<IntegrationConnectionConfig>,
) -> Element {
    rsx! {
        div {
            class: "flex items-center gap-2",

            label {
                class: "label-text cursor-pointer grow text-sm text-base-content",
                "Synchronize Slack mentions"
            }
            div {
                class: "relative inline-block",
                input {
                    r#type: "checkbox",
                    class: "switch switch-primary switch-outline peer",
                    oninput: move |event| {
                        on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                            message_config: SlackMessageConfig {
                                sync_enabled: event.value() == "true",
                                ..config().message_config
                            },
                            ..config()
                        }))
                    },
                    checked: config().message_config.sync_enabled
                }
                span {
                    class: "icon-[tabler--check] text-primary absolute start-1 top-1 hidden size-4 peer-checked:block"
                }
                span {
                    class: "icon-[tabler--x] text-neutral absolute end-1 top-1 block size-4 peer-checked:hidden"
                }
            }
        }
    }
}

#[component]
fn SlackExtensionConfiguration(
    config: ReadSignal<SlackConfig>,
    context: ReadSignal<Option<Option<SlackContext>>>,
    provider_user_id: ReadSignal<Option<Option<String>>>,
    on_config_change: EventHandler<IntegrationConnectionConfig>,
) -> Element {
    let slack_context = context().flatten();
    let expected_user_id = provider_user_id().flatten();
    let extension_enabled = config().message_config.extension_enabled;

    let bridge_status = use_resource(move || async move {
        if !extension_enabled {
            return None;
        }
        let api_base_url = get_api_base_url().ok()?;
        let result: Result<SlackBridgeStatus, _> = call_api(
            Method::GET,
            &api_base_url,
            "slack-bridge/status",
            None::<()>,
            None,
        )
        .await;
        match result {
            Ok(status) => Some(status),
            Err(err) => {
                error!("Failed to fetch bridge status: {err}");
                None
            }
        }
    });

    rsx! {
        div {
            class: "flex items-center gap-2",

            label {
                class: "label-text cursor-pointer grow text-sm text-base-content",
                "Enable browser extension bridge for Slack actions"
            }
            div {
                class: "relative inline-block",
                input {
                    r#type: "checkbox",
                    class: "switch switch-primary switch-outline peer",
                    oninput: move |event| {
                        on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                            message_config: SlackMessageConfig {
                                extension_enabled: event.value() == "true",
                                ..config().message_config
                            },
                            ..config()
                        }))
                    },
                    checked: config().message_config.extension_enabled
                }
                span {
                    class: "icon-[tabler--check] text-primary absolute start-1 top-1 hidden size-4 peer-checked:block"
                }
                span {
                    class: "icon-[tabler--x] text-neutral absolute end-1 top-1 block size-4 peer-checked:hidden"
                }
            }
        }

        p {
            class: "text-xs text-base-content/60",
            "When enabled, deleting or unsubscribing from Slack thread notifications will "
            "queue actions for the browser extension to execute using your Slack session."
        }

        if extension_enabled {
            div {
                class: "mt-4 flex flex-col gap-2 rounded-md bg-base-200 p-3 text-xs",

                div {
                    class: "flex items-center gap-2",
                    span { class: "font-medium text-base-content/70", "Last heartbeat:" }
                    if let Some(ref ctx) = slack_context {
                        if let Some(heartbeat) = ctx.last_extension_heartbeat_at {
                            {
                                let age_secs = (chrono::Utc::now() - heartbeat).num_seconds();
                                let is_stale = age_secs > 120;
                                let formatted = heartbeat
                                    .with_timezone(&Local)
                                    .to_rfc3339_opts(SecondsFormat::Secs, true);
                                rsx! {
                                    span {
                                        class: if is_stale { "text-warning" } else { "text-success" },
                                        "{formatted} ({age_secs}s ago)"
                                    }
                                }
                            }
                        } else {
                            span { class: "text-warning", "No heartbeat detected" }
                        }
                    } else {
                        span { class: "text-base-content/50", "No extension data available" }
                    }
                }

                if let Some(Some(ref status)) = *bridge_status.read() {
                    div {
                        class: "flex items-center gap-2",
                        span { class: "font-medium text-base-content/70", "Connection status:" }
                        if !status.extension_connected {
                            span { class: "text-warning",
                                "Extension not polling. Check it is installed and running."
                            }
                        } else if let Some(ref ctx) = slack_context {
                            if ctx.extension_credentials.is_empty() {
                                span { class: "text-warning",
                                    "Extension is polling but no Slack tab detected. Open app.slack.com in your browser, or grant the extension permission to access the tab."
                                }
                            } else if !status.team_id_match {
                                {
                                    let ext_teams = ctx.extension_credentials.iter()
                                        .map(|c| c.team_id.0.as_str())
                                        .collect::<Vec<_>>().join(", ");
                                    rsx! {
                                        span { class: "text-warning",
                                            "Workspace mismatch: extension sees team {ext_teams}, but the integration expects {ctx.team_id.0}."
                                        }
                                    }
                                }
                            } else if !status.user_id_match {
                                {
                                    let matching_cred = ctx.extension_credentials.iter()
                                        .find(|c| c.team_id == ctx.team_id);
                                    let ext_uid = matching_cred.map(|c| c.user_id.as_str()).unwrap_or("unknown");
                                    let expected_uid = expected_user_id.as_deref().unwrap_or("unknown");
                                    rsx! {
                                        span { class: "text-warning",
                                            "User mismatch: extension sees user {ext_uid}, but the integration expects {expected_uid}."
                                        }
                                    }
                                }
                            } else {
                                span { class: "text-success", "Connected and ready" }
                            }
                        } else {
                            span { class: "text-base-content/50", "No extension data available" }
                        }
                    }

                    div {
                        class: "flex items-center gap-2",
                        span { class: "font-medium text-base-content/70", "Pending actions:" }
                        span {
                            class: "text-base-content",
                            "{status.pending_actions_count}"
                        }
                    }

                    div {
                        class: "flex items-center gap-2",
                        span { class: "font-medium text-base-content/70", "Failed actions (retrying):" }
                        span {
                            class: if status.failed_actions_count > 0 { "text-warning" } else { "text-base-content" },
                            "{status.failed_actions_count}"
                        }
                    }
                }
            }
        }
    }
}
