#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsEnvelope, Icon};
use slack_morphism::prelude::*;

use universal_inbox::notification::{
    integrations::slack::{
        SlackChannelDetails, SlackFileCommentDetails, SlackFileDetails, SlackGroupDetails,
        SlackImDetails, SlackMessageDetails,
    },
    NotificationWithTask,
};

use crate::components::integrations::slack::icons::SlackNotificationIcon;

#[component]
pub fn SlackNotificationDisplay<'a>(
    cx: Scope,
    notif: &'a NotificationWithTask,
    slack_push_event_callback: SlackPushEventCallback,
) -> Element {
    let title = markdown::to_html(&notif.title);
    // : SlackStarsItem::Message { channel, .. },
    let subtitle = match &slack_push_event_callback.event {
        SlackEventCallbackBody::StarAdded(SlackStarAddedEvent { item, .. })
        | SlackEventCallbackBody::StarRemoved(SlackStarRemovedEvent { item, .. }) => match item {
            SlackStarsItem::Message(SlackStarsItemMessage { channel, .. })
            | SlackStarsItem::File(SlackStarsItemFile { channel, .. })
            | SlackStarsItem::FileComment(SlackStarsItemFileComment { channel, .. })
            | SlackStarsItem::Channel(SlackStarsItemChannel { channel, .. })
            | SlackStarsItem::Im(SlackStarsItemIm { channel, .. }) => format!("#{channel}"),
            SlackStarsItem::Group(SlackStarsItemGroup { group, .. }) => format!("@{group}"),
        },
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

                span { dangerous_inner_html: "{title}" }
                span {
                    class: "flex gap-2 text-xs text-gray-400",
                    "{subtitle}"
                }
            }
        }
    }
}

#[component]
pub fn SlackEventDetailsDisplay<'a>(
    cx: Scope,
    slack_push_event_callback: &'a SlackPushEventCallback,
) -> Element {
    let item = (match &slack_push_event_callback.event {
        SlackEventCallbackBody::StarAdded(SlackStarAddedEvent { item, .. })
        | SlackEventCallbackBody::StarRemoved(SlackStarRemovedEvent { item, .. }) => Some(item),
        _ => None,
    })?;

    let item_icon = match &item {
        SlackStarsItem::Message { .. } => render! { Icon { class: "h-5 w-5", icon: BsEnvelope } },
        SlackStarsItem::File { .. } => render! { Icon { class: "h-5 w-5", icon: BsEnvelope } },
        SlackStarsItem::FileComment { .. } => {
            render! { Icon { class: "h-5 w-5", icon: BsEnvelope } }
        }
        SlackStarsItem::Channel { .. } => render! { Icon { class: "h-5 w-5", icon: BsEnvelope } },
        SlackStarsItem::Im { .. } => render! { Icon { class: "h-5 w-5", icon: BsEnvelope } },
        SlackStarsItem::Group { .. } => render! { Icon { class: "h-5 w-5", icon: BsEnvelope } },
    };

    render! {
        div {
            class: "flex items-center gap-2",

            item_icon
        }
    }
}

#[component]
pub fn SlackMessageDetailsDisplay<'a>(
    cx: Scope,
    _slack_message: &'a SlackMessageDetails,
) -> Element {
    render! {
        div {
            class: "flex items-center gap-2",

        }
    }
}

#[component]
pub fn SlackFileDetailsDisplay<'a>(cx: Scope, _slack_file: &'a SlackFileDetails) -> Element {
    render! {
        div {
            class: "flex items-center gap-2",

        }
    }
}

#[component]
pub fn SlackFileCommentDetailsDisplay<'a>(
    cx: Scope,
    _slack_file_comment: &'a SlackFileCommentDetails,
) -> Element {
    render! {
        div {
            class: "flex items-center gap-2",

        }
    }
}

#[component]
pub fn SlackChannelDetailsDisplay<'a>(
    cx: Scope,
    _slack_channel: &'a SlackChannelDetails,
) -> Element {
    render! {
    div {
    class: "flex items-center gap-2",

        }
    }
}

#[component]
pub fn SlackImDetailsDisplay<'a>(cx: Scope, _slack_im: &'a SlackImDetails) -> Element {
    render! {
        div {
            class: "flex items-center gap-2",

        }
    }
}

#[component]
pub fn SlackGroupDetailsDisplay<'a>(cx: Scope, _slack_group: &'a SlackGroupDetails) -> Element {
    render! {
        div {
            class: "flex items-center gap-2",

        }
    }
}
