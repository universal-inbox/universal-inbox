#![allow(non_snake_case)]

use dioxus::prelude::dioxus_core::use_drop;
use dioxus::prelude::*;
#[cfg(feature = "web")]
use dioxus::web::WebEventExt;
use dioxus_free_icons::{Icon, icons::bs_icons::BsChatText};
use serde_json::json;
use slack_morphism::SlackReactionName;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::slack::{
            SlackConfig, SlackMessageConfig, SlackReactionConfig, SlackStarConfig,
            SlackSyncTaskConfig, SlackSyncType,
        },
    },
    task::{PresetDueDate, ProjectSummary, TaskPriority},
    utils::emoji::replace_emoji_code_with_emoji,
};

#[cfg(feature = "web")]
use crate::services::flyonui::{forget_flyonui_tabs_element, init_flyonui_tabs_element};
use crate::{
    components::{
        floating_label_inputs::{FloatingLabelInputSearchSelect, FloatingLabelSelect},
        flyonui::tooltip::{Tooltip, TooltipPlacement},
        integrations::slack::icons::SlackNotificationIcon,
    },
    config::get_api_base_url,
    model::UniversalInboxUIModel,
};

#[component]
pub fn SlackProviderConfiguration(
    config: ReadSignal<SlackConfig>,
    ui_model: Signal<UniversalInboxUIModel>,
    on_config_change: EventHandler<IntegrationConnectionConfig>,
) -> Element {
    let emoji = replace_emoji_code_with_emoji(config().reaction_config.reaction_name.0.as_str())
        .unwrap_or("👀".to_string());
    #[cfg(feature = "web")]
    let mut mounted_element: Signal<Option<web_sys::Element>> = use_signal(|| None);
    #[cfg(not(feature = "web"))]
    let mut mounted_element: Signal<Option<()>> = use_signal(|| None);

    use_drop(move || {
        #[cfg(feature = "web")]
        {
            if let Some(element) = mounted_element() {
                forget_flyonui_tabs_element(&element);
            }
        }
    });

    rsx! {
        div {
            class: "flex flex-col",

            nav {
                role: "tablist",
                class: "tabs tabs-bordered tabs-sm flex-wrap",
                onmounted: move |element| {
                    #[cfg(feature = "web")]
                    {
                        let web_element = element.as_web_event();
                        init_flyonui_tabs_element(&web_element);
                        mounted_element.set(Some(web_element));
                    }
                },

                button {
                    class: "tab active-tab:tab-active active flex items-center gap-2 rounded-b-none",
                    "type": "button",
                    role: "tab",
                    "data-tab": "#slack-config-tab-saved-for-later",
                    SlackNotificationIcon { class: "h-3 w-3" }
                    "Saved for later"
                }

                button {
                    class: "tab active-tab:tab-active flex items-center gap-2 rounded-b-none",
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
            }

            div {
                div {
                    id: "slack-config-tab-saved-for-later",
                    role: "tabpanel",
                    class: "bg-base-100 border-base-300 p-6 rounded-b-md flex flex-col gap-2",
                    SlackStarConfiguration { config, ui_model, on_config_change }
                }

                div {
                    id: "slack-config-tab-reaction",
                    role: "tabpanel",
                    class: "bg-base-100 border-base-300 p-6 rounded-b-md flex flex-col gap-2 hidden",
                    SlackReactionConfiguration { config, ui_model, on_config_change }
                }

                div {
                    id: "slack-config-tab-mention",
                    role: "tabpanel",
                    class: "bg-base-100 border-base-300 p-6 rounded-b-md flex flex-col gap-2 hidden",
                    SlackMessageConfiguration { config, ui_model, on_config_change }
                }
            }
        }
    }
}

#[component]
fn SlackStarConfiguration(
    config: ReadSignal<SlackConfig>,
    ui_model: Signal<UniversalInboxUIModel>,
    on_config_change: EventHandler<IntegrationConnectionConfig>,
) -> Element {
    let mut default_priority = use_signal(|| Some(TaskPriority::P4));
    let mut default_due_at: Signal<Option<PresetDueDate>> = use_signal(|| None);
    let mut default_project: Signal<Option<ProjectSummary>> = use_signal(|| None);
    let mut task_config_enabled = use_signal(|| false);
    use_effect(move || {
        if let SlackSyncType::AsTasks(config) = config().star_config.sync_type {
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
    let api_base_url = get_api_base_url().unwrap();
    let as_tasks_disabled =
        !config().star_config.sync_enabled || !ui_model.read().is_task_actions_enabled;

    rsx! {
        div {
            class: "flex items-center gap-2",
            label {
                class: "label-text cursor-pointer grow text-sm text-base-content",
                "Synchronize Slack \"saved for later\" items"
            }
            div {
                class: "relative inline-block",
                input {
                    r#type: "checkbox",
                    class: "switch switch-primary switch-outline peer",
                    oninput: move |event| {
                        on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                            star_config: SlackStarConfig {
                                sync_enabled: event.value() == "true",
                                ..config().star_config
                            },
                            ..config()
                        }))
                    },
                    checked: config().star_config.sync_enabled
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
                "for": "slack-saved-for-later-as-notifications",
                "Synchronize Slack \"saved for later\" items as notifications"
            }
            input {
                id: "slack-saved-for-later-as-notifications",
                disabled: !config().star_config.sync_enabled,
                r#type: "radio",
                class: "radio radio-soft radio-sm",
                name: "star-sync-type",
                oninput: move |_event| {
                    on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                        star_config: SlackStarConfig {
                            sync_type: SlackSyncType::AsNotifications,
                            ..config().star_config
                        },
                        ..config()
                    }))
                },
                checked: config().star_config.sync_type == SlackSyncType::AsNotifications
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
                        "for": "slack-saved-for-later-as-tasks",
                        "Synchronize Slack \"saved for later\" items as tasks"
                    }
                    input {
                        id: "slack-saved-for-later-as-tasks",
                        disabled: as_tasks_disabled,
                        name: "star-sync-type",
                        class: "radio radio-soft radio-sm",
                        r#type: "radio",
                        oninput: move |_event| {
                            on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                                star_config: SlackStarConfig {
                                    sync_type: SlackSyncType::AsTasks(match &config().star_config.sync_type {
                                        SlackSyncType::AsTasks(config) => config.clone(),
                                        _ => Default::default(),
                                    }),
                                    ..config().star_config
                                },
                                ..config()
                            }))
                        },
                        checked: !(config().star_config.sync_type == SlackSyncType::AsNotifications)
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
                        name: "star-project-search-input".to_string(),
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
                                star_config: SlackStarConfig {
                                    sync_type: SlackSyncType::AsTasks(match &config().star_config.sync_type {
                                        SlackSyncType::AsTasks(config) => SlackSyncTaskConfig {
                                            target_project: project.clone(),
                                            ..config.clone()
                                        },
                                        _ => Default::default(),
                                    }),
                                    ..config().star_config
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
                                star_config: SlackStarConfig {
                                    sync_type: SlackSyncType::AsTasks(match &config().star_config.sync_type {
                                        SlackSyncType::AsTasks(task_config) => SlackSyncTaskConfig {
                                            default_due_at,
                                            ..task_config.clone()
                                        },
                                        _ => SlackSyncTaskConfig {
                                            default_due_at,
                                            ..Default::default()
                                        }
                                    }),
                                    ..config().star_config
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
                                star_config: SlackStarConfig {
                                    sync_type: SlackSyncType::AsTasks(match &config().star_config.sync_type {
                                        SlackSyncType::AsTasks(task_config) => SlackSyncTaskConfig {
                                            default_priority: priority.unwrap_or_default(),
                                            ..task_config.clone()
                                        },
                                        _ => SlackSyncTaskConfig {
                                            default_priority: priority.unwrap_or_default(),
                                            ..Default::default()
                                        },
                                    }),
                                    ..config().star_config
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
fn SlackReactionConfiguration(
    config: ReadSignal<SlackConfig>,
    ui_model: Signal<UniversalInboxUIModel>,
    on_config_change: EventHandler<IntegrationConnectionConfig>,
) -> Element {
    let mut default_emoji = use_signal(|| "eyes".to_string());
    let mut default_priority = use_signal(|| Some(TaskPriority::P4));
    let mut default_due_at: Signal<Option<PresetDueDate>> = use_signal(|| None);
    let mut default_project: Signal<Option<ProjectSummary>> = use_signal(|| None);
    let mut task_config_enabled = use_signal(|| false);
    use_memo(move || {
        *default_emoji.write() = config().reaction_config.reaction_name.0.clone();
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
            FloatingLabelSelect::<String> {
                label: None,
                class: "max-w-xs",
                name: "reaction-name-input".to_string(),
                required: true,
                default_value: default_emoji(),
                on_select: move |reaction: Option<String>| {
                    on_config_change.call(IntegrationConnectionConfig::Slack(SlackConfig {
                        reaction_config: SlackReactionConfig {
                            reaction_name: SlackReactionName(reaction.unwrap_or("eyes".to_string())),
                            ..config().reaction_config
                        },
                        ..config()
                    }));
                },

                option { selected: default_emoji() == *"eyes", value: "eyes", "👀 :eyes:" }
                option { selected: default_emoji() == *"raising_hand", value: "raising_hand", "🙋 :raising_hand:" }
                option { selected: default_emoji() == *"inbox_tray", value: "inbox_tray", "📥 :inbox_tray:" }
                option { selected: default_emoji() == *"white_check_mark", value: "white_check_mark", "✅ :white_check_mark:" }
                option { selected: default_emoji() == *"bookmark", value: "bookmark", "🔖 :bookmark:" }
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
