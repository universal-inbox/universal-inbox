#![allow(non_snake_case)]

use dioxus::prelude::*;
use log::error;
use reqwest::Method;
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
        provider::IntegrationProviderKind,
    },
    slack_bridge::SlackBridgeStatus,
    task::{PresetDueDate, ProjectSummary, TaskPriority},
};

use crate::{
    components::{
        emoji_search_field::EmojiSearchField,
        flyonui::tooltip::{Tooltip, TooltipPlacement},
        project_search_field::ProjectSearchField,
        settings_controls::{SegmentedChoice, SegmentedChoiceOption, SettingRow},
        task_manager_picker::{ProviderIcon, resolve_task_manager_kind},
        ui::{
            TaskMgrOption, TaskMgrValue, ToggleSize, ToggleSwitch, UISelect, UISelectOption,
            preset_due_date_options, priority_select_renderers, task_priority_options,
        },
    },
    config::get_api_base_url,
    model::{LoadState, UniversalInboxUIModel},
    services::{
        api::call_api, integration_connection_service::TASK_SERVICE_INTEGRATION_CONNECTIONS,
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
    let active_tab = use_signal(|| "reaction".to_string());

    let tab_options = vec![
        SegmentedChoiceOption {
            value: "reaction".to_string(),
            label: "Reaction".to_string(),
            icon_class: Some("icon-[lucide--smile-plus]".to_string()),
        },
        SegmentedChoiceOption {
            value: "mention".to_string(),
            label: "Mention".to_string(),
            icon_class: Some("icon-[lucide--message-square]".to_string()),
        },
        SegmentedChoiceOption {
            value: "extension".to_string(),
            label: "Extension".to_string(),
            icon_class: Some("icon-[lucide--puzzle]".to_string()),
        },
    ];

    rsx! {
        div {
            class: "flex flex-col gap-3",

            SegmentedChoice {
                options: tab_options,
                selected: active_tab(),
                on_change: move |value: String| active_tab.clone().set(value),
                aria_label: "Slack configuration tabs".to_string(),
            }

            div {
                class: if active_tab() == "reaction" { "settings-tab-panel" } else { "settings-tab-panel hidden" },
                SlackReactionConfiguration { config, connection_id, ui_model, on_config_change }
            }

            div {
                class: if active_tab() == "mention" { "settings-tab-panel" } else { "settings-tab-panel hidden" },
                SlackMessageConfiguration { config, on_config_change }
            }

            div {
                class: if active_tab() == "extension" { "settings-tab-panel" } else { "settings-tab-panel hidden" },
                SlackExtensionConfiguration { config, context, provider_user_id, on_config_change }
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
    // Bridging signals: emoji selects need full `SlackEmojiSuggestion` for the
    // search-select trigger, but the rest of the form tracks the bare name.
    let mut default_emoji_suggestion: Signal<Option<SlackEmojiSuggestion>> = use_signal(|| None);
    let mut default_completion_emoji_suggestion: Signal<Option<SlackEmojiSuggestion>> =
        use_signal(|| None);
    use_effect(move || {
        let name = default_emoji();
        let current = default_emoji_suggestion
            .peek()
            .as_ref()
            .map(|s| s.name.clone());
        if Some(name.clone()) != current {
            default_emoji_suggestion.set(Some(SlackEmojiSuggestion {
                name: name.clone(),
                display_name: name,
            }));
        }
    });
    use_effect(move || {
        let name = default_completion_emoji();
        let current = default_completion_emoji_suggestion
            .peek()
            .as_ref()
            .map(|s| s.name.clone());
        if name != current {
            default_completion_emoji_suggestion.set(name.map(|n| SlackEmojiSuggestion {
                name: n.clone(),
                display_name: n,
            }));
        }
    });
    let mut default_priority = use_signal(|| Some(TaskPriority::P4));
    let mut default_due_at: Signal<Option<PresetDueDate>> = use_signal(|| None);
    let mut default_project: Signal<Option<ProjectSummary>> = use_signal(|| None);
    let mut default_task_manager_provider_kind: Signal<Option<IntegrationProviderKind>> =
        use_signal(|| None);
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
            *default_task_manager_provider_kind.write() = config.task_manager_provider_kind;
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
    let show_task_manager_select = matches!(
        &*TASK_SERVICE_INTEGRATION_CONNECTIONS.read(),
        LoadState::Loaded(connections) if connections.len() >= 2
    );
    let api_base_url = get_api_base_url().unwrap();
    let (priority_render_value, priority_render_option) = priority_select_renderers();
    let as_tasks_disabled =
        !config().reaction_config.sync_enabled || !ui_model.read().is_task_actions_enabled;

    let sync_type_value = if config().reaction_config.sync_type == SlackSyncType::AsNotifications {
        "notifications"
    } else {
        "tasks"
    };
    let sync_type_options: Vec<SegmentedChoiceOption> = vec![
        ("notifications".to_string(), "Notifications".to_string()).into(),
        ("tasks".to_string(), "Tasks".to_string()).into(),
    ];

    rsx! {
        SettingRow {
            label: rsx! { "Synchronize Slack reacted items" },
            ToggleSwitch {
                size: ToggleSize::Md,
                checked: config().reaction_config.sync_enabled,
                onchange: move |new_value: bool| {
                    on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                        reaction_config: SlackReactionConfig {
                            sync_enabled: new_value,
                            ..config().reaction_config
                        },
                        ..config()
                    }))
                },
            }
        }

        SettingRow {
            label: rsx! { "Emoji reaction to synchronize" },
            EmojiSearchField {
                api_base_url: api_base_url.clone(),
                connection_id,
                selected: default_emoji_suggestion,
                on_change: move |emoji: Option<SlackEmojiSuggestion>| {
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
                name: "reaction-name-input".to_string(),
                allow_clear: false,
                width: "260px".to_string(),
            }
        }

        SettingRow {
            label: rsx! { "Set an emoji reaction when the task is completed" },
            ToggleSwitch {
                size: ToggleSize::Md,
                checked: config().reaction_config.completion_reaction_name.is_some(),
                disabled: !config().reaction_config.sync_enabled,
                onchange: move |new_value: bool| {
                    let completion_reaction_name = if new_value {
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
            }
        }

        div {
            class: "collapse transition-[height] duration-300 {completion_reaction_collapse_style}",
            SettingRow {
                label: rsx! { "Emoji reaction to set on completion" },
                EmojiSearchField {
                    api_base_url: api_base_url.clone(),
                    connection_id,
                    selected: default_completion_emoji_suggestion,
                    on_change: move |emoji: Option<SlackEmojiSuggestion>| {
                        on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                            reaction_config: SlackReactionConfig {
                                completion_reaction_name: emoji.map(|e| SlackReactionName(e.name)),
                                ..config().reaction_config
                            },
                            ..config()
                        }));
                    },
                    name: "completion-reaction-name-input".to_string(),
                    disabled: !config().reaction_config.sync_enabled,
                    width: "260px".to_string(),
                }
            }
        }

        Tooltip {
            placement: TooltipPlacement::Bottom,
            disabled: !as_tasks_disabled,
            tooltip_class: "tooltip-error",
            text: "A task management service must be connected to enable this feature",

            SettingRow {
                label: rsx! { "Sync reacted items as" },
                SegmentedChoice {
                    options: sync_type_options,
                    selected: sync_type_value.to_string(),
                    disabled: !config().reaction_config.sync_enabled,
                    aria_label: "Sync reacted items as".to_string(),
                    on_change: move |value: String| {
                        let new_sync_type = if value == "notifications" {
                            SlackSyncType::AsNotifications
                        } else {
                            SlackSyncType::AsTasks(match &config().reaction_config.sync_type {
                                SlackSyncType::AsTasks(c) => c.clone(),
                                _ => Default::default(),
                            })
                        };
                        on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                            reaction_config: SlackReactionConfig {
                                sync_type: new_sync_type,
                                ..config().reaction_config
                            },
                            ..config()
                        }))
                    },
                }
            }
        }

        div {
            class: "collapse transition-[height] duration-300 {collapse_style} flex flex-col",

            SettingRow {
                label: rsx! { "Project to assign synchronized tasks to" },
                {
                    rsx! {
                        ProjectSearchField {
                            api_base_url: api_base_url.clone(),
                            selected_project: default_project,
                            provider_kind: Some(resolve_task_manager_kind(default_task_manager_provider_kind())),
                            on_change: move |project: Option<ProjectSummary>| {
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
                            },
                            name: "reaction-project-search-input".to_string(),
                            disabled: !ui_model.read().is_task_actions_enabled,
                            width: "260px".to_string(),
                        }
                    }
                }
            }

            SettingRow {
                label: rsx! { "Due date to assign to synchronized tasks" },
                UISelect::<PresetDueDate> {
                    value: default_due_at,
                    options: preset_due_date_options(),
                    on_change: move |default_due_at| {
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
                    placeholder: "Pick a due date…".to_string(),
                    allow_clear: true,
                    disabled: !ui_model.read().is_task_actions_enabled,
                    width: "260px".to_string(),
                    name: "task-due-at-input".to_string(),
                }
            }

            SettingRow {
                label: rsx! { "Priority to assign to synchronized tasks" },
                UISelect::<TaskPriority> {
                    value: default_priority,
                    options: task_priority_options(),
                    on_change: move |priority: Option<TaskPriority>| {
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
                    placeholder: "Pick a priority…".to_string(),
                    disabled: !ui_model.read().is_task_actions_enabled,
                    width: "260px".to_string(),
                    name: "task-priority-input".to_string(),
                    render_value: priority_render_value,
                    render_option: priority_render_option,
                }
            }

            if show_task_manager_select {
                SettingRow {
                    label: rsx! { "Task manager to sync with" },
                    UISelect::<IntegrationProviderKind> {
                        value: default_task_manager_provider_kind,
                        options: vec![
                            UISelectOption::new(IntegrationProviderKind::Todoist, "Todoist"),
                            UISelectOption::new(IntegrationProviderKind::TickTick, "TickTick"),
                        ],
                        on_change: move |task_manager_provider_kind| {
                            *default_project.write() = None;
                            on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                                reaction_config: SlackReactionConfig {
                                    sync_type: SlackSyncType::AsTasks(match &config().reaction_config.sync_type {
                                        SlackSyncType::AsTasks(task_config) => SlackSyncTaskConfig {
                                            task_manager_provider_kind,
                                            target_project: None,
                                            ..task_config.clone()
                                        },
                                        _ => SlackSyncTaskConfig {
                                            task_manager_provider_kind,
                                            target_project: None,
                                            ..Default::default()
                                        },
                                    }),
                                    ..config().reaction_config
                                },
                                ..config()
                            }));
                        },
                        placeholder: "Pick a task manager…".to_string(),
                        disabled: !ui_model.read().is_task_actions_enabled,
                        width: "260px".to_string(),
                        name: "reaction-task-manager-input".to_string(),
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
                    }
                }
            }
        }
    }
}

#[component]
fn SlackMessageConfiguration(
    config: ReadSignal<SlackConfig>,
    on_config_change: EventHandler<IntegrationConnectionConfig>,
) -> Element {
    rsx! {
        SettingRow {
            label: rsx! { "Synchronize Slack mentions" },
            ToggleSwitch {
                size: ToggleSize::Md,
                checked: config().message_config.sync_enabled,
                onchange: move |new_value: bool| {
                    on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                        message_config: SlackMessageConfig {
                            sync_enabled: new_value,
                            ..config().message_config
                        },
                        ..config()
                    }))
                },
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
        SettingRow {
            label: rsx! { "Enable browser extension bridge for Slack actions" },
            ToggleSwitch {
                size: ToggleSize::Md,
                checked: config().message_config.extension_enabled,
                onchange: move |new_value: bool| {
                    on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                        message_config: SlackMessageConfig {
                            extension_enabled: new_value,
                            ..config().message_config
                        },
                        ..config()
                    }))
                },
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
