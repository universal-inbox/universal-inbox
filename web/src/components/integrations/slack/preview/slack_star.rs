#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{
    task::Task,
    third_party::integrations::slack::{SlackStar, SlackStarredItem},
};

use crate::components::integrations::slack::preview::{
    channel::SlackChannelPreview, file::SlackFilePreview, file_comment::SlackFileCommentPreview,
    group::SlackGroupPreview, im::SlackImPreview, message::SlackMessagePreview,
};

#[component]
pub fn SlackStarTaskPreview(
    slack_star: ReadOnlySignal<SlackStar>,
    task: ReadOnlySignal<Task>,
) -> Element {
    match slack_star().starred_item {
        SlackStarredItem::SlackChannel(slack_channel) => rsx! {
            SlackChannelPreview { slack_channel, title: task().title }
        },
        SlackStarredItem::SlackFile(slack_file) => rsx! {
            SlackFilePreview { slack_file, title: task().title }
        },
        SlackStarredItem::SlackFileComment(slack_file_comment) => rsx! {
            SlackFileCommentPreview { slack_file_comment, title: task().title }
        },
        SlackStarredItem::SlackGroup(slack_group) => rsx! {
            SlackGroupPreview { slack_group, title: task().title }
        },
        SlackStarredItem::SlackIm(slack_im) => rsx! {
            SlackImPreview { slack_im, title: task().title }
        },
        SlackStarredItem::SlackMessage(slack_message) => rsx! {
            SlackMessagePreview { slack_message, title: task().title }
        },
    }
}
