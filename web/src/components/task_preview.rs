#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{
    task::Task,
    third_party::item::{ThirdPartyItemData, ThirdPartyItemKind},
};

use crate::{
    components::{
        integrations::{
            icons::TaskIcon,
            linear::preview::issue::LinearIssuePreview,
            slack::preview::{
                slack_reaction::SlackReactionTaskPreview, slack_star::SlackStarTaskPreview,
            },
            todoist::preview::TodoistTaskPreview,
        },
        tasks_list::{TaskListContext, get_task_list_item_action_buttons},
    },
    model::UniversalInboxUIModel,
    services::{task_service::TaskCommand, user_service::CONNECTED_USER},
};

#[component]
pub fn TaskPreview(
    ui_model: Signal<UniversalInboxUIModel>,
    task: ReadSignal<Task>,
    expand_details: ReadSignal<bool>,
    is_help_enabled: ReadSignal<bool>,
    tasks_count: ReadSignal<usize>,
) -> Element {
    let task_service = use_coroutine_handle::<TaskCommand>();
    let is_read_only = CONNECTED_USER
        .read()
        .as_ref()
        .map(|ctx| ctx.subscription.is_read_only)
        .unwrap_or(false);
    let context = use_memo(move || TaskListContext {
        is_task_actions_enabled: ui_model.read().is_task_actions_enabled,
        is_read_only,
        task_service,
    });
    use_context_provider(move || context);
    let shortcut_visibility_style = use_memo(move || {
        if is_help_enabled() {
            "visible"
        } else {
            "invisible"
        }
    });
    let previous_button_style = if ui_model.read().selected_task_index.unwrap_or_default() == 0 {
        "btn-disabled"
    } else {
        ""
    };
    let next_button_style =
        if ui_model.read().selected_task_index.unwrap_or_default() == tasks_count() - 1 {
            "btn-disabled"
        } else {
            ""
        };
    let task_type = match task().source_item.kind() {
        ThirdPartyItemKind::TodoistItem => "Task",
        ThirdPartyItemKind::SlackStar => "Saved for later message",
        ThirdPartyItemKind::SlackReaction => "Reaction",
        ThirdPartyItemKind::LinearIssue => "Issue",
        _ => "Task",
    };

    rsx! {
        div {
            class: "flex flex-col w-full h-full",

            div {
                class: "relative w-full",

                span {
                    class: "{shortcut_visibility_style} kbd kbd-xs z-50 absolute left-0",
                    "▼ j"
                }
                span {
                    class: "{shortcut_visibility_style} kbd kbd-xs z-50 absolute right-0",
                    "▲ k"
                }

                nav {
                    class: "tabs tabs-bordered w-full pb-2",
                    role: "tablist",

                    button {
                        class: "tab active-tab:tab-active active w-full",
                        "data-tab": "#source-task-tab",
                        role: "tab",
                        div {
                            class: "flex gap-2 items-center text-base-content",
                            TaskIcon { class: "h-5 w-5", kind: task().kind }
                            "{task_type}"
                        }
                    }
                }
            }

            button {
                class: "btn btn-text absolute left-0 lg:hidden",
                onclick: move |_| ui_model.write().selected_task_index = None,
                span { class: "icon-[tabler--arrow-left] size-8" }
            }

            if shortcut_visibility_style == "visible" {
                span {
                    class: "{shortcut_visibility_style} kbd kbd-xs z-50",
                    "e: expand/collapse"
                }
            }

            TaskDetailsPreview { task, expand_details }

            div {
                class: "flex flex-col w-full gap-2 lg:hidden",

                hr { class: "text-gray-200" }
                div {
                    class: "flex w-full justify-center text-sm text-base-content/50",

                    span { "{ui_model.read().selected_task_index.unwrap_or_default() + 1} of {tasks_count()}" }
                }

                div {
                    class: "flex w-full",
                    button {
                        "type": "button",
                        class: "btn btn-text btn-square btn-lg {previous_button_style}",
                        "aria-label": "Previous notification",
                        onclick: move |_| {
                            let mut model = ui_model.write();
                            model.selected_task_index = Some(model.selected_task_index.unwrap_or_default() - 1);
                        },
                        span { class: "icon-[tabler--chevron-left] size-5 rtl:rotate-180" }
                    }

                    for btn in get_task_list_item_action_buttons(
                        task,
                        false,
                        Some("btn btn-square btn-primary btn-lg".to_string()),
                        Some("flex-1".to_string())) {
                        { btn }
                    }

                    button {
                        "type": "button",
                        class: "btn btn-text btn-square btn-lg {next_button_style}",
                        "aria-label": "Next notification",
                        onclick: move |_| {
                            let mut model = ui_model.write();
                            model.selected_task_index = Some(model.selected_task_index.unwrap_or_default() + 1);
                        },
                        span { class: "icon-[tabler--chevron-right] size-5 rtl:rotate-180" }
                    }
                }
            }

        }
    }
}

#[component]
pub fn TaskDetailsPreview(task: ReadSignal<Task>, expand_details: ReadSignal<bool>) -> Element {
    match task().source_item.data {
        ThirdPartyItemData::TodoistItem(todoist_item) => rsx! {
            TodoistTaskPreview { todoist_item: *todoist_item, task }
        },
        ThirdPartyItemData::SlackStar(slack_star) => rsx! {
            SlackStarTaskPreview { slack_star: *slack_star, task }
        },
        ThirdPartyItemData::SlackReaction(slack_reaction) => rsx! {
            SlackReactionTaskPreview { slack_reaction: *slack_reaction, task }
        },
        ThirdPartyItemData::LinearIssue(linear_issue) => rsx! {
            LinearIssuePreview {
                linear_issue: *linear_issue,
                linear_notification: None,
                expand_details
            }
        },
        _ => rsx! {},
    }
}
