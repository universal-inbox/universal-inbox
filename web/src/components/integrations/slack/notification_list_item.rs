#![allow(non_snake_case)]

use chrono::{DateTime, Local};
use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{BsChatText, BsSlack},
    Icon,
};
use slack_morphism::SlackChannelInfo;

use universal_inbox::{
    notification::NotificationWithTask,
    third_party::{
        integrations::slack::{
            SlackChannelDetails, SlackFileCommentDetails, SlackFileDetails, SlackGroupDetails,
            SlackImDetails, SlackMessageDetails, SlackMessageRender, SlackReaction,
            SlackReactionItem, SlackStarItem, SlackThread,
        },
        item::ThirdPartyItemData,
    },
    utils::emoji::replace_emoji_code_with_emoji,
    HasHtmlUrl,
};

use crate::components::{
    integrations::slack::{
        icons::SlackNotificationIcon, SlackMessageActorDisplay, SlackTeamDisplay, SlackUserDisplay,
    },
    list::{ListContext, ListItem},
    notifications_list::{get_notification_list_item_action_buttons, TaskHint},
};

#[component]
pub fn SlackStarNotificationListItem(
    notification: ReadOnlySignal<NotificationWithTask>,
    is_selected: ReadOnlySignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    rsx! {
        SlackNotificationListItem {
            notification,
            subicon: rsx! { SlackNotificationIcon { class: "h-5 w-5 min-w-5" } },
            is_selected,
            on_select,
        }
    }
}

#[component]
pub fn SlackReactionNotificationListItem(
    notification: ReadOnlySignal<NotificationWithTask>,
    slack_reaction: ReadOnlySignal<SlackReaction>,
    is_selected: ReadOnlySignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let emoji = replace_emoji_code_with_emoji(&slack_reaction().name.0).unwrap_or("👀".to_string());

    rsx! {
        SlackNotificationListItem {
            notification,
            subicon: rsx! { span { class: "h-5 w-5 min-w-5", "{emoji}" } },
            is_selected,
            on_select,
        }
    }
}

#[component]
pub fn SlackThreadNotificationListItem(
    notification: ReadOnlySignal<NotificationWithTask>,
    is_selected: ReadOnlySignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    rsx! {
        SlackNotificationListItem {
            notification,
            subicon: rsx! { Icon { class: "h-5 w-5 min-w-5", icon: BsChatText } },
            is_selected,
            on_select,
        }
    }
}

#[component]
pub fn SlackNotificationListItem(
    notification: ReadOnlySignal<NotificationWithTask>,
    subicon: Option<Element>,
    is_selected: ReadOnlySignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let notification_updated_at = use_memo(move || {
        Into::<DateTime<Local>>::into(notification().updated_at)
            .format("%Y-%m-%d %H:%M")
            .to_string()
    });
    let list_context = use_context::<Memo<ListContext>>();
    let link = notification().get_html_url();

    rsx! {
        ListItem {
            key: "{notification().id}",
            title: "{notification().title}",
            subtitle: rsx! { SlackNotificationSubtitle { notification } },
            link,
            icon: rsx! { Icon { class: "h-5 w-5", icon: BsSlack }, TaskHint { task: notification().task } },
            subicon,
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
    fn channel_str(channel: &SlackChannelInfo) -> String {
        if let Some(channel_name) = &channel.name {
            format!("#{}", channel_name)
        } else {
            format!("#{}", channel.id)
        }
    }
    let subtitle = match notification().source_item.data {
        ThirdPartyItemData::SlackStar(slack_star) => match slack_star.item {
            SlackStarItem::SlackMessage(item) => channel_str(&item.channel),
            SlackStarItem::SlackFile(item) => channel_str(&item.channel),
            SlackStarItem::SlackFileComment(item) => channel_str(&item.channel),
            SlackStarItem::SlackChannel(item) => channel_str(&item.channel),
            SlackStarItem::SlackIm(item) => channel_str(&item.channel),
            SlackStarItem::SlackGroup(item) => channel_str(&item.channel),
        },
        ThirdPartyItemData::SlackReaction(slack_reaction) => match slack_reaction.item {
            SlackReactionItem::SlackMessage(item) => channel_str(&item.channel),
            SlackReactionItem::SlackFile(item) => channel_str(&item.channel),
        },
        ThirdPartyItemData::SlackThread(slack_thread) => {
            if slack_thread.messages.len() > 1 {
                let first_message_text = slack_thread
                    .messages
                    .first()
                    .render_title(slack_thread.references.clone());
                format!(
                    "Replied to `{}` in {}",
                    first_message_text,
                    channel_str(&slack_thread.channel)
                )
            } else {
                format!("in {}", channel_str(&slack_thread.channel))
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
    match notification().source_item.data {
        ThirdPartyItemData::SlackStar(slack_star) => match slack_star.item {
            SlackStarItem::SlackMessage(slack_message) => rsx! {
                SlackMessageListItemDetails { slack_message: *slack_message }
            },
            SlackStarItem::SlackFile(slack_file) => rsx! {
                SlackFileListItemDetails { slack_file: *slack_file }
            },
            SlackStarItem::SlackChannel(slack_channel) => rsx! {
                SlackChannelListItemDetails { slack_channel: *slack_channel }
            },
            SlackStarItem::SlackFileComment(slack_file_comment) => rsx! {
                SlackFileCommentListItemDetails { slack_file_comment: *slack_file_comment }
            },
            SlackStarItem::SlackIm(slack_im) => rsx! {
                SlackImListItemDetails { slack_im: *slack_im }
            },
            SlackStarItem::SlackGroup(slack_group) => rsx! {
                SlackGroupListItemDetails { slack_group: *slack_group }
            },
        },
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
pub fn SlackThreadListItemDetails(slack_thread: ReadOnlySignal<SlackThread>) -> Element {
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
pub fn SlackMessageListItemDetails(slack_message: ReadOnlySignal<SlackMessageDetails>) -> Element {
    rsx! {
        SlackTeamDisplay { team: slack_message().team }
        SlackMessageActorDisplay { sender: slack_message().sender }
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
