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
pub fn SlackNotificationDisplay<'a>(
    cx: Scope,
    notif: &'a NotificationWithTask,
    slack_push_event_callback: SlackPushEventCallback,
) -> Element {
    let subtitle = match &notif.details {
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

    render! {
        div {
            class: "flex items-center gap-2",

            SlackNotificationIcon {
                class: "h-5 w-5 min-w-5",
                slack_push_event_callback: slack_push_event_callback,
            }

            div {
                class: "flex flex-col grow",

                Markdown { text: notif.title.clone() }
                span {
                    class: "flex gap-2 text-xs text-gray-400",
                    "{subtitle}"
                }
            }
        }
    }
}

#[component]
pub fn SlackMessageDetailsDisplay<'a>(
    cx: Scope,
    slack_message: &'a SlackMessageDetails,
) -> Element {
    render! {
        div {
            class: "flex items-center gap-2",

            SlackTeamDisplay { team: &slack_message.team }
            SlackMessageActorDisplay { slack_message: &slack_message }
        }
    }
}

#[component]
pub fn SlackFileDetailsDisplay<'a>(cx: Scope, slack_file: &'a SlackFileDetails) -> Element {
    render! {
        div {
            class: "flex items-center gap-2",

            SlackTeamDisplay { team: &slack_file.team }
            if let Some(ref user) = slack_file.sender {
                render! { SlackUserDisplay { user: user } }
            }
        }
    }
}

#[component]
pub fn SlackFileCommentDetailsDisplay<'a>(
    cx: Scope,
    slack_file_comment: &'a SlackFileCommentDetails,
) -> Element {
    render! {
        div {
            class: "flex items-center gap-2",

            SlackTeamDisplay { team: &slack_file_comment.team }
            if let Some(ref user) = slack_file_comment.sender {
                render! { SlackUserDisplay { user: user } }
            }
        }
    }
}

#[component]
pub fn SlackChannelDetailsDisplay<'a>(
    cx: Scope,
    slack_channel: &'a SlackChannelDetails,
) -> Element {
    render! {
        div {
            class: "flex items-center gap-2",

            SlackTeamDisplay { team: &slack_channel.team }
        }
    }
}

#[component]
pub fn SlackImDetailsDisplay<'a>(cx: Scope, slack_im: &'a SlackImDetails) -> Element {
    render! {
        div {
            class: "flex items-center gap-2",

            SlackTeamDisplay { team: &slack_im.team }
        }
    }
}

#[component]
pub fn SlackGroupDetailsDisplay<'a>(cx: Scope, slack_group: &'a SlackGroupDetails) -> Element {
    render! {
        div {
            class: "flex items-center gap-2",

            SlackTeamDisplay { team: &slack_group.team }
        }
    }
}
