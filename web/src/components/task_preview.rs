#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{task::Task, third_party::item::ThirdPartyItemData};

use crate::components::integrations::{
    linear::preview::issue::LinearIssuePreview,
    slack::preview::slack_reaction::SlackReactionTaskPreview,
    slack::preview::slack_star::SlackStarTaskPreview, todoist::preview::TodoistTaskPreview,
};

#[component]
pub fn TaskPreview(task: ReadOnlySignal<Task>) -> Element {
    rsx! {
        div {
            class: "flex flex-col gap-4 w-full",

            TaskDetailsPreview { task },
        }
    }
}

#[component]
pub fn TaskDetailsPreview(task: ReadOnlySignal<Task>) -> Element {
    match task().source_item.data {
        ThirdPartyItemData::TodoistItem(todoist_item) => {
            rsx! { TodoistTaskPreview { todoist_item, task } }
        }
        ThirdPartyItemData::SlackStar(slack_star) => {
            rsx! { SlackStarTaskPreview { slack_star, task } }
        }
        ThirdPartyItemData::SlackReaction(slack_reaction) => {
            rsx! { SlackReactionTaskPreview { slack_reaction, task } }
        }
        ThirdPartyItemData::LinearIssue(linear_issue) => {
            rsx! { LinearIssuePreview { linear_issue, linear_notification: None } }
        }
    }
}
