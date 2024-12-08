#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{task::Task, third_party::item::ThirdPartyItemData};

use crate::components::integrations::{
    linear::preview::issue::LinearIssuePreview,
    slack::preview::{slack_reaction::SlackReactionTaskPreview, slack_star::SlackStarTaskPreview},
    todoist::preview::TodoistTaskPreview,
};

#[component]
pub fn TaskPreview(
    task: ReadOnlySignal<Task>,
    expand_details: ReadOnlySignal<bool>,
    is_help_enabled: ReadOnlySignal<bool>,
) -> Element {
    let shortcut_visibility_style = use_memo(move || {
        if is_help_enabled() {
            "visible"
        } else {
            "invisible"
        }
    });

    rsx! {
        div {
            class: "flex flex-col gap-4 w-full",

            if shortcut_visibility_style == "visible" {
                div {
                    class: "flex flex-row w-full",
                    span {
                        class: "{shortcut_visibility_style} indicator-item indicator-top indicator-start badge text-xs text-gray-400 z-50",
                        "▼ j"
                    }
                    div { class: "grow" }
                    span {
                        class: "{shortcut_visibility_style} indicator-item indicator-top indicator-start badge text-xs text-gray-400 z-50",
                        "e: expand/collapse"
                    }
                    div { class: "grow" }
                    span {
                        class: "{shortcut_visibility_style} indicator-item indicator-top indicator-start badge text-xs text-gray-400 z-50",
                        "▲ k"
                    }
                }
            }

            TaskDetailsPreview { task, expand_details },
        }
    }
}

#[component]
pub fn TaskDetailsPreview(
    task: ReadOnlySignal<Task>,
    expand_details: ReadOnlySignal<bool>,
) -> Element {
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
