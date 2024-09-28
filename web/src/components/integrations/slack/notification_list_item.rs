#![allow(non_snake_case)]

use chrono::{DateTime, Local};
use dioxus::prelude::*;

use dioxus_free_icons::{icons::bs_icons::BsSlack, Icon};
use slack_morphism::events::SlackPushEventCallback;
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
    list::{ListContext, ListItem},
    notifications_list::{get_notification_list_item_action_buttons, TaskHint},
};

#[component]
pub fn SlackNotificationListItem(
    notification: ReadOnlySignal<NotificationWithTask>,
    slack_push_event_callback: SlackPushEventCallback,
    is_selected: ReadOnlySignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let notification_updated_at = use_memo(move || {
        Into::<DateTime<Local>>::into(notification().updated_at)
            .format("%Y-%m-%d %H:%M")
            .to_string()
    });
    let list_context = use_context::<Memo<ListContext>>();

    rsx! {
        ListItem {
            key: "{notification().id}",
            title: "{notification().title}",
            subtitle: rsx! { SlackNotificationSubtitle { notification } },
            icon: rsx! { Icon { class: "h-5 w-5", icon: BsSlack }, TaskHint { task: notification().task } },
            subicon: rsx! { SlackNotificationIcon { class: "h-5 w-5 min-w-5" } },
            action_buttons: get_notification_list_item_action_buttons(
                notification,
                list_context().show_shortcut,
            ),
            is_selected,
            on_select,

            SlackNotificationListItemDetails { notification }

            span { class: "text-gray-400 whitespace-nowrap text-xs font-mono", "{notification_updated_at}" }
        }
    }
}

#[component]
pub fn SlackNotificationSubtitle(notification: ReadOnlySignal<NotificationWithTask>) -> Element {
    let subtitle = match notification().details {
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
        span {
            class: "flex gap-2 text-xs text-gray-400",
            "{subtitle}"
        }
    }
}

#[component]
fn SlackNotificationListItemDetails(notification: ReadOnlySignal<NotificationWithTask>) -> Element {
    match notification().details {
        Some(NotificationDetails::SlackMessage(slack_message)) => rsx! {
            SlackMessageListItemDetails { slack_message }
        },
        Some(NotificationDetails::SlackChannel(slack_channel)) => rsx! {
            SlackChannelListItemDetails { slack_channel }
        },
        Some(NotificationDetails::SlackFile(slack_file)) => rsx! {
            SlackFileListItemDetails { slack_file }
        },
        Some(NotificationDetails::SlackFileComment(slack_file_comment)) => rsx! {
            SlackFileCommentListItemDetails { slack_file_comment }
        },
        Some(NotificationDetails::SlackIm(slack_im)) => rsx! {
            SlackImListItemDetails { slack_im }
        },
        Some(NotificationDetails::SlackGroup(slack_group)) => rsx! {
            SlackGroupListItemDetails { slack_group }
        },
        _ => None,
    }
}

#[component]
pub fn SlackMessageListItemDetails(slack_message: ReadOnlySignal<SlackMessageDetails>) -> Element {
    rsx! {
        SlackTeamDisplay { team: slack_message().team }
        SlackMessageActorDisplay { slack_message }
    }
}

#[component]
pub fn SlackFileListItemDetails(slack_file: ReadOnlySignal<SlackFileDetails>) -> Element {
    rsx! {
        SlackTeamDisplay { team: slack_file().team }
        if let Some(user) = slack_file().sender {
            SlackUserDisplay { user }
        }
    }
}

#[component]
pub fn SlackFileCommentListItemDetails(
    slack_file_comment: ReadOnlySignal<SlackFileCommentDetails>,
) -> Element {
    rsx! {
        SlackTeamDisplay { team: slack_file_comment().team }
        if let Some(user) = slack_file_comment().sender {
            SlackUserDisplay { user }
        }
    }
}

#[component]
pub fn SlackChannelListItemDetails(slack_channel: ReadOnlySignal<SlackChannelDetails>) -> Element {
    rsx! {
        SlackTeamDisplay { team: slack_channel().team }
    }
}

#[component]
pub fn SlackImListItemDetails(slack_im: ReadOnlySignal<SlackImDetails>) -> Element {
    rsx! {
        SlackTeamDisplay { team: slack_im().team }
    }
}

#[component]
pub fn SlackGroupListItemDetails(slack_group: ReadOnlySignal<SlackGroupDetails>) -> Element {
    rsx! {
        SlackTeamDisplay { team: slack_group().team }
    }
}
