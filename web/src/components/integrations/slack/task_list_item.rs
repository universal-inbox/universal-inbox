#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{Icon, icons::bs_icons::BsSlack};

use slack_morphism::SlackChannelInfo;
use universal_inbox::{
    HasHtmlUrl,
    task::Task,
    third_party::integrations::slack::{
        SlackFileDetails, SlackMessageDetails, SlackReaction, SlackReactionItem, SlackStar,
        SlackStarItem,
    },
    utils::emoji::replace_emoji_code_with_emoji,
};

use crate::{
    components::{
        integrations::slack::{
            icons::SlackNotificationIcon,
            notification_list_item::{
                SlackChannelListItemDetails, SlackFileCommentListItemDetails,
                SlackFileListItemDetails, SlackGroupListItemDetails, SlackImListItemDetails,
                SlackMessageListItemDetails,
            },
        },
        list::{ListContext, ListItem},
        notifications_list::TaskHint,
        tasks_list::get_task_list_item_action_buttons,
    },
    utils::format_elapsed_time,
};

#[component]
pub fn SlackStarTaskListItem(
    task: ReadOnlySignal<Task>,
    slack_star: ReadOnlySignal<SlackStar>,
    is_selected: ReadOnlySignal<bool>,
    on_select: EventHandler<()>,
) -> Element {
    let task_updated_at = use_memo(move || format_elapsed_time(task().updated_at));
    let list_context = use_context::<Memo<ListContext>>();
    let link = task().get_html_url();

    rsx! {
        ListItem {
            key: "{task().id}",
            title: "{task().title}",
            subtitle: rsx! { SlackStarTaskSubtitle { slack_star } },
            link,
            icon: rsx! {
                Icon { class: "h-5 w-5", icon: BsSlack },
                TaskHint { task: Some(task()) }
            },
            subicon: rsx! { SlackNotificationIcon { class: "h-5 w-5 min-w-5" } },
            action_buttons: get_task_list_item_action_buttons(
                task,
                list_context().show_shortcut,
                None,
                None
            ),
            is_selected,
            on_select,

            SlackStarTaskListItemDetails { slack_star }

            span { class: "text-base-content/50 whitespace-nowrap text-xs font-mono", "{task_updated_at}" }
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
    let task_updated_at = use_memo(move || format_elapsed_time(task().updated_at));
    let list_context = use_context::<Memo<ListContext>>();
    let reaction_emoji =
        replace_emoji_code_with_emoji(&slack_reaction().name.0).unwrap_or("👀".to_string());
    let link = task().get_html_url();

    rsx! {
        ListItem {
            key: "{task().id}",
            title: "{task().title}",
            subtitle: rsx! { SlackReactionTaskSubtitle { slack_reaction } },
            link,
            icon: rsx! {
                Icon { class: "h-5 w-5", icon: BsSlack }
                TaskHint { task: Some(task()) }
            },
            subicon: rsx! { span { class: "h-5 w-5 min-w-5", "{reaction_emoji}" } },
            action_buttons: get_task_list_item_action_buttons(
                task,
                list_context().show_shortcut,
                None,
                None,
            ),
            is_selected,
            on_select,

            SlackReactionTaskListItemDetails { slack_reaction }

            span { class: "text-base-content/50 whitespace-nowrap text-xs font-mono", "{task_updated_at}" }
        }
    }
}

#[component]
pub fn SlackStarTaskSubtitle(slack_star: ReadOnlySignal<SlackStar>) -> Element {
    let subtitle = match slack_star().item {
        SlackStarItem::SlackMessage(item) => channel_str(&item.channel),
        SlackStarItem::SlackFile(item) => channel_str(&item.channel),
        SlackStarItem::SlackFileComment(item) => channel_str(&item.channel),
        SlackStarItem::SlackChannel(item) => channel_str(&item.channel),
        SlackStarItem::SlackIm(item) => channel_str(&item.channel),
        SlackStarItem::SlackGroup(item) => channel_str(&item.channel),
    };

    rsx! {
        span {
            class: "flex gap-2 text-xs text-base-content/50",
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
            class: "flex gap-2 text-xs text-base-content/50",
            "{subtitle}"
        }
    }
}

#[component]
fn SlackStarTaskListItemDetails(slack_star: ReadOnlySignal<SlackStar>) -> Element {
    match slack_star().item {
        SlackStarItem::SlackMessage(slack_message) => rsx! {
            SlackMessageListItemDetails { slack_message: *slack_message }
        },
        SlackStarItem::SlackFile(slack_file) => rsx! {
            SlackFileListItemDetails { slack_file: *slack_file }
        },
        SlackStarItem::SlackFileComment(slack_file_comment) => rsx! {
            SlackFileCommentListItemDetails { slack_file_comment: *slack_file_comment }
        },
        SlackStarItem::SlackChannel(slack_channel) => rsx! {
            SlackChannelListItemDetails { slack_channel: *slack_channel }
        },
        SlackStarItem::SlackIm(slack_im) => rsx! {
            SlackImListItemDetails { slack_im: *slack_im }
        },
        SlackStarItem::SlackGroup(slack_group) => rsx! {
            SlackGroupListItemDetails { slack_group: *slack_group }
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

fn channel_str(channel: &SlackChannelInfo) -> String {
    if let Some(channel_name) = &channel.name {
        format!("#{}", channel_name)
    } else {
        format!("#{}", channel.id)
    }
}
