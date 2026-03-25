#![allow(non_snake_case)]

use dioxus::prelude::*;
use slack_morphism::SlackChannelInfo;

use universal_inbox::{
    notification::{NotificationStatus, NotificationWithTask},
    third_party::{
        integrations::slack::{
            SlackFileDetails, SlackMessageDetails, SlackMessageRender, SlackReaction,
            SlackReactionItem, SlackThread,
        },
        item::ThirdPartyItemData,
    },
};

use universal_inbox::utils::emoji::replace_emoji_code_with_emoji;

use crate::{
    components::{
        integrations::slack::{SlackMessageActorDisplay, SlackTeamDisplay, SlackUserDisplay},
        list::ListItem,
    },
    utils::format_elapsed_time,
};

#[component]
pub fn SlackReactionNotificationListItem(
    notification: ReadSignal<NotificationWithTask>,
    slack_reaction: ReadSignal<SlackReaction>,
    is_selected: ReadSignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let emoji = replace_emoji_code_with_emoji(&slack_reaction().name.0).unwrap_or("👀".to_string());

    rsx! {
        SlackNotificationListItem {
            notification,
            state_tag: rsx! { span { class: "tag", "{emoji}" } },
            is_selected,
            on_select,
        }
    }
}

#[component]
pub fn SlackThreadNotificationListItem(
    notification: ReadSignal<NotificationWithTask>,
    is_selected: ReadSignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    rsx! {
        SlackNotificationListItem {
            notification,
            state_tag: rsx! {},
            is_selected,
            on_select,
        }
    }
}

#[component]
pub fn SlackNotificationListItem(
    notification: ReadSignal<NotificationWithTask>,
    state_tag: Element,
    is_selected: ReadSignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let notification_updated_at = use_memo(move || format_elapsed_time(notification().updated_at));
    let is_unread = notification().status == NotificationStatus::Unread;

    rsx! {
        ListItem {
            key: "{notification().id}",
            linked_task: notification().task,
            title: "{notification().title}",
            subtitle: rsx! {
                SlackNotificationSubtitle { notification }
                { state_tag }
            },
            time: "{notification_updated_at}",
            icon: rsx! {
                div {
                    class: "w-full h-full flex items-center justify-center rounded-[inherit] bg-[var(--ui-surface)] border border-[var(--ui-border)]",
                    span { class: "icon-[logos--slack-icon] size-4" }
                }
            },
            meta_icon: rsx! { span { class: "icon-[lucide--hash] w-full h-full" } },
            is_selected,
            is_unread,
            provider: Some("slack"),
            on_select,
        }
    }
}

#[component]
pub fn SlackNotificationSubtitle(notification: ReadSignal<NotificationWithTask>) -> Element {
    fn channel_str(channel: &SlackChannelInfo) -> String {
        if let Some(channel_name) = &channel.name {
            format!("#{}", channel_name)
        } else {
            format!("#{}", channel.id)
        }
    }
    let subtitle = match notification().source_item.data {
        ThirdPartyItemData::SlackReaction(slack_reaction) => match slack_reaction.item {
            SlackReactionItem::SlackMessage(item) => channel_str(&item.channel),
            SlackReactionItem::SlackFile(item) => channel_str(&item.channel),
        },
        ThirdPartyItemData::SlackThread(slack_thread) => {
            if slack_thread.messages.len() > 1 {
                format!("Replied in {}", channel_str(&slack_thread.channel))
            } else {
                format!("in {}", channel_str(&slack_thread.channel))
            }
        }
        _ => "".to_string(),
    };

    rsx! {
        span {
            class: "ui-nrow-meta-text",
            "{subtitle}"
        }
    }
}

#[component]
fn SlackNotificationListItemDetails(notification: ReadSignal<NotificationWithTask>) -> Element {
    match notification().source_item.data {
        ThirdPartyItemData::SlackReaction(slack_reaction) => match slack_reaction.item {
            SlackReactionItem::SlackMessage(slack_message) => rsx! {
                SlackMessageListItemDetails { slack_message }
            },
            SlackReactionItem::SlackFile(slack_file) => rsx! {
                SlackFileListItemDetails { slack_file }
            },
        },
        ThirdPartyItemData::SlackThread(slack_thread) => rsx! {
            SlackThreadListItemDetails { slack_thread: *slack_thread }
        },
        _ => rsx! {},
    }
}

#[component]
pub fn SlackThreadListItemDetails(slack_thread: ReadSignal<SlackThread>) -> Element {
    let slack_thread = slack_thread();
    let first_unread_message = slack_thread.first_unread_message();
    let sender = first_unread_message.get_sender(&slack_thread.sender_profiles);

    rsx! {
        SlackTeamDisplay { team: slack_thread.team }
        if let Some(sender) = sender {
            SlackMessageActorDisplay { sender }
        }
    }
}

#[component]
pub fn SlackMessageListItemDetails(slack_message: ReadSignal<SlackMessageDetails>) -> Element {
    rsx! {
        SlackTeamDisplay { team: slack_message().team }
        SlackMessageActorDisplay { sender: slack_message().sender }
    }
}

#[component]
pub fn SlackFileListItemDetails(slack_file: ReadSignal<SlackFileDetails>) -> Element {
    rsx! {
        SlackTeamDisplay { team: slack_file().team }
        if let Some(user) = slack_file().sender {
            SlackUserDisplay { user }
        }
    }
}
