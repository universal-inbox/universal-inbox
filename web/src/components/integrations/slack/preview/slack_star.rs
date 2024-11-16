#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{
    task::Task,
    third_party::integrations::slack::{SlackStar, SlackStarItem},
};

use crate::components::integrations::slack::{
    icons::SlackNotificationIcon,
    preview::{
        channel::SlackChannelPreview, file::SlackFilePreview,
        file_comment::SlackFileCommentPreview, group::SlackGroupPreview, im::SlackImPreview,
        message::SlackMessagePreview,
    },
};

#[component]
pub fn SlackStarTaskPreview(
    slack_star: ReadOnlySignal<SlackStar>,
    task: ReadOnlySignal<Task>,
) -> Element {
    match slack_star().item {
        SlackStarItem::SlackChannel(slack_channel) => rsx! {
            SlackChannelPreview {
                slack_channel: *slack_channel,
                title: task().title,
                icon: rsx! { SlackNotificationIcon { class: "h-5 w-5" } },
            }
        },
        SlackStarItem::SlackFile(slack_file) => rsx! {
            SlackFilePreview {
                slack_file: *slack_file,
                title: task().title,
                icon: rsx! { SlackNotificationIcon { class: "h-5 w-5" } },
            }
        },
        SlackStarItem::SlackFileComment(slack_file_comment) => rsx! {
            SlackFileCommentPreview {
                slack_file_comment: *slack_file_comment,
                title: task().title,
                icon: rsx! { SlackNotificationIcon { class: "h-5 w-5" } },
            }
        },
        SlackStarItem::SlackGroup(slack_group) => rsx! {
            SlackGroupPreview {
                slack_group: *slack_group,
                title: task().title,
                icon: rsx! { SlackNotificationIcon { class: "h-5 w-5" } },
            }
        },
        SlackStarItem::SlackIm(slack_im) => rsx! {
            SlackImPreview {
                slack_im: *slack_im,
                title: task().title,
                icon: rsx! { SlackNotificationIcon { class: "h-5 w-5" } },
            }
        },
        SlackStarItem::SlackMessage(slack_message) => rsx! {
            SlackMessagePreview {
                slack_message: *slack_message,
                title: task().title,
                icon: rsx! { SlackNotificationIcon { class: "h-5 w-5" } },
            }
        },
    }
}
