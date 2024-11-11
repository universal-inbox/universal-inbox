#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::third_party::integrations::slack::{
    SlackChannelDetails, SlackFileCommentDetails, SlackFileDetails, SlackGroupDetails,
    SlackImDetails, SlackMessageDetails,
};

use crate::components::integrations::slack::{
    SlackMessageActorDisplay, SlackTeamDisplay, SlackUserDisplay,
};

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
