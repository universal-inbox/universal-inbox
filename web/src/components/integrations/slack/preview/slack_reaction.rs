#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{
    task::Task,
    third_party::integrations::slack::{SlackReaction, SlackReactionItem},
};

use crate::components::{
    integrations::slack::preview::{file::SlackFilePreview, message::SlackMessagePreview},
    task_preview::task_manager_title_subtitle,
};

#[component]
pub fn SlackReactionTaskPreview(
    slack_reaction: ReadSignal<SlackReaction>,
    task: ReadSignal<Task>,
) -> Element {
    // Show the title Slack seeded the task with, so the name reads the same as
    // it does in Slack, and hand any renamed task-manager title to the subtitle
    // so both names stay visible.
    let source_title = slack_reaction().render_task_title();
    let subtitle = task_manager_title_subtitle(&task(), &source_title);

    match slack_reaction().item {
        SlackReactionItem::SlackFile(slack_file) => rsx! {
            SlackFilePreview {
                slack_file,
                title: source_title,
                subtitle,
            }
        },
        SlackReactionItem::SlackMessage(slack_message) => rsx! {
            SlackMessagePreview {
                slack_message,
                title: source_title,
                subtitle,
            }
        },
    }
}
