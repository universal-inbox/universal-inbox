#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{Icon, icons::bs_icons::BsSlack};

use universal_inbox::{
    HasHtmlUrl,
    task::Task,
    third_party::integrations::slack::{
        SlackFileDetails, SlackMessageDetails, SlackReaction, SlackReactionItem,
    },
    utils::emoji::replace_emoji_code_with_emoji,
};

use crate::{
    components::{
        integrations::slack::notification_list_item::{
            SlackFileListItemDetails, SlackMessageListItemDetails,
        },
        list::{ListContext, ListItem},
        notifications_list::TaskHint,
        tasks_list::get_task_list_item_action_buttons,
    },
    utils::format_elapsed_time,
};

#[component]
pub fn SlackReactionTaskListItem(
    task: ReadSignal<Task>,
    slack_reaction: ReadSignal<SlackReaction>,
    is_selected: ReadSignal<bool>,
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
pub fn SlackReactionTaskSubtitle(slack_reaction: ReadSignal<SlackReaction>) -> Element {
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
fn SlackReactionTaskListItemDetails(slack_reaction: ReadSignal<SlackReaction>) -> Element {
    match slack_reaction().item {
        SlackReactionItem::SlackMessage(slack_message) => rsx! {
            SlackMessageListItemDetails { slack_message }
        },
        SlackReactionItem::SlackFile(slack_file) => rsx! {
            SlackFileListItemDetails { slack_file }
        },
    }
}
