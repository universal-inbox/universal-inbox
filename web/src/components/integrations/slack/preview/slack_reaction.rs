#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{
    task::Task,
    third_party::integrations::slack::{SlackReaction, SlackReactionItem},
};

use crate::components::integrations::slack::{
    icons::SlackNotificationIcon,
    preview::{file::SlackFilePreview, message::SlackMessagePreview},
};

#[component]
pub fn SlackReactionTaskPreview(
    slack_reaction: ReadOnlySignal<SlackReaction>,
    task: ReadOnlySignal<Task>,
) -> Element {
    match slack_reaction().item {
        SlackReactionItem::SlackFile(slack_file) => rsx! {
            SlackFilePreview {
                slack_file,
                title: task().title,
                icon: rsx! { SlackNotificationIcon { class: "h-5 w-5 min-w-5" } },
            }
        },
        SlackReactionItem::SlackMessage(slack_message) => rsx! {
            SlackMessagePreview {
                slack_message,
                title: task().title,
                icon: rsx! { SlackNotificationIcon { class: "h-5 w-5 min-w-5" } },
            }
        },
    }
}
