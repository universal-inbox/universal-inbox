#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{
    task::Task,
    third_party::integrations::slack::{SlackReaction, SlackReactionItem},
};

use crate::components::integrations::slack::preview::{
    file::SlackFilePreview, message::SlackMessagePreview,
};

#[component]
pub fn SlackReactionTaskPreview(
    slack_reaction: ReadSignal<SlackReaction>,
    task: ReadSignal<Task>,
) -> Element {
    match slack_reaction().item {
        SlackReactionItem::SlackFile(slack_file) => rsx! {
            SlackFilePreview {
                slack_file,
                title: task().title,
            }
        },
        SlackReactionItem::SlackMessage(slack_message) => rsx! {
            SlackMessagePreview {
                slack_message,
                title: task().title,
            }
        },
    }
}
