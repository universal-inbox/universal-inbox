#![allow(non_snake_case)]

use dioxus::prelude::*;
use slack_morphism::prelude::*;

use universal_inbox::notification::{
    integrations::slack::{
        SlackChannelDetails, SlackFileCommentDetails, SlackFileDetails, SlackGroupDetails,
        SlackImDetails, SlackMessageDetails,
    },
    NotificationDetails, NotificationWithTask,
};

use crate::components::{
    integrations::slack::{
        icons::SlackNotificationIcon, SlackMessageActorDisplay, SlackTeamDisplay, SlackUserDisplay,
    },
    markdown::Markdown,
};

#[component]
pub fn SlackNotificationDisplay(
    notif: ReadOnlySignal<NotificationWithTask>,
    slack_push_event_callback: SlackPushEventCallback,
) -> Element {
    let subtitle = match notif().details {
        Some(NotificationDetails::SlackMessage(SlackMessageDetails { channel, .. }))
        | Some(NotificationDetails::SlackFile(SlackFileDetails { channel, .. }))
        | Some(NotificationDetails::SlackFileComment(SlackFileCommentDetails {
            channel, ..
        }))
        | Some(NotificationDetails::SlackChannel(SlackChannelDetails { channel, .. }))
        | Some(NotificationDetails::SlackIm(SlackImDetails { channel, .. }))
        | Some(NotificationDetails::SlackGroup(SlackGroupDetails { channel, .. })) => {
            if let Some(channel_name) = &channel.name {
                format!("#{}", channel_name)
            } else {
                format!("#{}", channel.id)
            }
        }
        _ => "".to_string(),
    };

    rsx! {
        div {
            class: "flex items-center gap-2",

            SlackNotificationIcon {
                class: "h-5 w-5 min-w-5",
                slack_push_event_callback: slack_push_event_callback,
            }

            div {
                class: "flex flex-col grow",

                Markdown { text: notif().title.clone() }
                span {
                    class: "flex gap-2 text-xs text-gray-400",
                    "{subtitle}"
                }
            }
        }
    }
}

#[component]
pub fn SlackMessageDetailsDisplay(slack_message: ReadOnlySignal<SlackMessageDetails>) -> Element {
    rsx! {
        div {
            class: "flex items-center gap-2",

            SlackTeamDisplay { team: slack_message().team }
            SlackMessageActorDisplay { slack_message: slack_message }
        }
    }
}

#[component]
pub fn SlackFileDetailsDisplay(slack_file: ReadOnlySignal<SlackFileDetails>) -> Element {
    rsx! {
        div {
            class: "flex items-center gap-2",

            SlackTeamDisplay { team: slack_file().team }
            if let Some(user) = slack_file().sender {
                SlackUserDisplay { user: user }
            }
        }
    }
}

#[component]
pub fn SlackFileCommentDetailsDisplay(
    slack_file_comment: ReadOnlySignal<SlackFileCommentDetails>,
) -> Element {
    rsx! {
        div {
            class: "flex items-center gap-2",

            SlackTeamDisplay { team: slack_file_comment().team }
            if let Some(user) = slack_file_comment().sender {
                SlackUserDisplay { user: user }
            }
        }
    }
}

#[component]
pub fn SlackChannelDetailsDisplay(slack_channel: ReadOnlySignal<SlackChannelDetails>) -> Element {
    rsx! {
        div {
            class: "flex items-center gap-2",

            SlackTeamDisplay { team: slack_channel().team }
        }
    }
}

#[component]
pub fn SlackImDetailsDisplay(slack_im: ReadOnlySignal<SlackImDetails>) -> Element {
    rsx! {
        div {
            class: "flex items-center gap-2",

            SlackTeamDisplay { team: slack_im().team }
        }
    }
}

#[component]
pub fn SlackGroupDetailsDisplay(slack_group: ReadOnlySignal<SlackGroupDetails>) -> Element {
    rsx! {
        div {
            class: "flex items-center gap-2",

            SlackTeamDisplay { team: slack_group().team }
        }
    }
}
