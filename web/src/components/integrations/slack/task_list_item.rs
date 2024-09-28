#![allow(non_snake_case)]

use chrono::{DateTime, Local};
use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsSlack, Icon};

use universal_inbox::{
    notification::integrations::slack::{
        SlackChannelDetails, SlackFileCommentDetails, SlackFileDetails, SlackGroupDetails,
        SlackImDetails, SlackMessageDetails,
    },
    task::Task,
    third_party::integrations::slack::{SlackStar, SlackStarredItem},
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
pub fn SlackTaskListItem(
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
            subtitle: rsx! { SlackTaskSubtitle { slack_star } },
            icon: rsx! { Icon { class: "h-5 w-5", icon: BsSlack } },
            subicon: rsx! { SlackNotificationIcon { class: "h-5 w-5 min-w-5" } },
            action_buttons: get_task_list_item_action_buttons(
                task,
                list_context().show_shortcut,
            ),
            is_selected,
            on_select,

            SlackTaskListItemDetails { slack_star }

            span { class: "text-gray-400 whitespace-nowrap text-xs font-mono", "{task_updated_at}" }
        }
    }
}

#[component]
pub fn SlackTaskSubtitle(slack_star: ReadOnlySignal<SlackStar>) -> Element {
    let subtitle = match slack_star().starred_item {
        SlackStarredItem::SlackMessage(SlackMessageDetails { channel, .. })
        | SlackStarredItem::SlackFile(SlackFileDetails { channel, .. })
        | SlackStarredItem::SlackFileComment(SlackFileCommentDetails { channel, .. })
        | SlackStarredItem::SlackChannel(SlackChannelDetails { channel, .. })
        | SlackStarredItem::SlackIm(SlackImDetails { channel, .. })
        | SlackStarredItem::SlackGroup(SlackGroupDetails { channel, .. }) => {
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
fn SlackTaskListItemDetails(slack_star: ReadOnlySignal<SlackStar>) -> Element {
    match slack_star().starred_item {
        SlackStarredItem::SlackMessage(slack_message) => rsx! {
            SlackMessageListItemDetails { slack_message }
        },
        SlackStarredItem::SlackFile(slack_file) => rsx! {
            SlackFileListItemDetails { slack_file }
        },
        SlackStarredItem::SlackFileComment(slack_file_comment) => rsx! {
            SlackFileCommentListItemDetails { slack_file_comment }
        },
        SlackStarredItem::SlackChannel(slack_channel) => rsx! {
            SlackChannelListItemDetails { slack_channel }
        },
        SlackStarredItem::SlackIm(slack_im) => rsx! {
            SlackImListItemDetails { slack_im }
        },
        SlackStarredItem::SlackGroup(slack_group) => rsx! {
            SlackGroupListItemDetails { slack_group }
        },
    }
}
