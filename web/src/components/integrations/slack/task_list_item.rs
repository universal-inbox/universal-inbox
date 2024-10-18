#![allow(non_snake_case)]

use chrono::{DateTime, Local};
use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsSlack, Icon};

use universal_inbox::{
    task::Task,
    third_party::integrations::slack::{
        SlackChannelDetails, SlackFileCommentDetails, SlackFileDetails, SlackGroupDetails,
        SlackImDetails, SlackMessageDetails,
    },
    third_party::integrations::slack::{
        SlackReaction, SlackReactionItem, SlackStar, SlackStarItem,
    },
    utils::emoji::replace_emoji_code_with_emoji,
};

use crate::components::{
    integrations::slack::{
        icons::SlackNotificationIcon,
        notification_list_item::{
            SlackChannelListItemDetails, SlackFileCommentListItemDetails, SlackFileListItemDetails,
            SlackGroupListItemDetails, SlackImListItemDetails, SlackMessageListItemDetails,
        },
    },
    list::{ListContext, ListItem},
    tasks_list::get_task_list_item_action_buttons,
};

#[component]
pub fn SlackStarTaskListItem(
    task: ReadOnlySignal<Task>,
    slack_star: ReadOnlySignal<SlackStar>,
    is_selected: ReadOnlySignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let task_updated_at = use_memo(move || {
        Into::<DateTime<Local>>::into(task().updated_at)
            .format("%Y-%m-%d %H:%M")
            .to_string()
    });
    let list_context = use_context::<Memo<ListContext>>();

    rsx! {
        ListItem {
            key: "{task().id}",
            title: "{task().title}",
            subtitle: rsx! { SlackStarTaskSubtitle { slack_star } },
            icon: rsx! { Icon { class: "h-5 w-5", icon: BsSlack } },
            subicon: rsx! { SlackNotificationIcon { class: "h-5 w-5 min-w-5" } },
            action_buttons: get_task_list_item_action_buttons(
                task,
                list_context().show_shortcut,
            ),
            is_selected,
            on_select,

            SlackStarTaskListItemDetails { slack_star }

            span { class: "text-gray-400 whitespace-nowrap text-xs font-mono", "{task_updated_at}" }
        }
    }
}

#[component]
pub fn SlackReactionTaskListItem(
    task: ReadOnlySignal<Task>,
    slack_reaction: ReadOnlySignal<SlackReaction>,
    is_selected: ReadOnlySignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let task_updated_at = use_memo(move || {
        Into::<DateTime<Local>>::into(task().updated_at)
            .format("%Y-%m-%d %H:%M")
            .to_string()
    });
    let list_context = use_context::<Memo<ListContext>>();
    let reaction_emoji =
        replace_emoji_code_with_emoji(&slack_reaction().name.0).unwrap_or("👀".to_string());

    rsx! {
        ListItem {
            key: "{task().id}",
            title: "{task().title}",
            subtitle: rsx! { SlackReactionTaskSubtitle { slack_reaction } },
            icon: rsx! { Icon { class: "h-5 w-5", icon: BsSlack } },
            subicon: rsx! { span { class: "h-5 w-5 min-w-5", "{reaction_emoji}" } },
            action_buttons: get_task_list_item_action_buttons(
                task,
                list_context().show_shortcut,
            ),
            is_selected,
            on_select,

            SlackReactionTaskListItemDetails { slack_reaction }

            span { class: "text-gray-400 whitespace-nowrap text-xs font-mono", "{task_updated_at}" }
        }
    }
}

#[component]
pub fn SlackStarTaskSubtitle(slack_star: ReadOnlySignal<SlackStar>) -> Element {
    let subtitle = match slack_star().item {
        SlackStarItem::SlackMessage(SlackMessageDetails { channel, .. })
        | SlackStarItem::SlackFile(SlackFileDetails { channel, .. })
        | SlackStarItem::SlackFileComment(SlackFileCommentDetails { channel, .. })
        | SlackStarItem::SlackChannel(SlackChannelDetails { channel, .. })
        | SlackStarItem::SlackIm(SlackImDetails { channel, .. })
        | SlackStarItem::SlackGroup(SlackGroupDetails { channel, .. }) => {
            if let Some(channel_name) = &channel.name {
                format!("#{}", channel_name)
            } else {
                format!("#{}", channel.id)
            }
        }
    };

    rsx! {
        span {
            class: "flex gap-2 text-xs text-gray-400",
            "{subtitle}"
        }
    }
}

#[component]
pub fn SlackReactionTaskSubtitle(slack_reaction: ReadOnlySignal<SlackReaction>) -> Element {
    let subtitle = match slack_reaction().item {
        SlackReactionItem::SlackMessage(SlackMessageDetails { channel, .. })
        | SlackReactionItem::SlackFile(SlackFileDetails { channel, .. }) => {
            if let Some(channel_name) = &channel.name {
                format!("#{}", channel_name)
            } else {
                format!("#{}", channel.id)
            }
        }
    };

    rsx! {
        span {
            class: "flex gap-2 text-xs text-gray-400",
            "{subtitle}"
        }
    }
}

#[component]
fn SlackStarTaskListItemDetails(slack_star: ReadOnlySignal<SlackStar>) -> Element {
    match slack_star().item {
        SlackStarItem::SlackMessage(slack_message) => rsx! {
            SlackMessageListItemDetails { slack_message }
        },
        SlackStarItem::SlackFile(slack_file) => rsx! {
            SlackFileListItemDetails { slack_file }
        },
        SlackStarItem::SlackFileComment(slack_file_comment) => rsx! {
            SlackFileCommentListItemDetails { slack_file_comment }
        },
        SlackStarItem::SlackChannel(slack_channel) => rsx! {
            SlackChannelListItemDetails { slack_channel }
        },
        SlackStarItem::SlackIm(slack_im) => rsx! {
            SlackImListItemDetails { slack_im }
        },
        SlackStarItem::SlackGroup(slack_group) => rsx! {
            SlackGroupListItemDetails { slack_group }
        },
    }
}

#[component]
fn SlackReactionTaskListItemDetails(slack_reaction: ReadOnlySignal<SlackReaction>) -> Element {
    match slack_reaction().item {
        SlackReactionItem::SlackMessage(slack_message) => rsx! {
            SlackMessageListItemDetails { slack_message }
        },
        SlackReactionItem::SlackFile(slack_file) => rsx! {
            SlackFileListItemDetails { slack_file }
        },
    }
}
