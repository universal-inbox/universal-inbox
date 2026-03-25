#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{
    task::Task,
    third_party::integrations::slack::{
        SlackFileDetails, SlackMessageDetails, SlackReaction, SlackReactionItem,
    },
};

use universal_inbox::utils::emoji::replace_emoji_code_with_emoji;

use crate::{
    components::{
        integrations::slack::notification_list_item::{
            SlackFileListItemDetails, SlackMessageListItemDetails,
        },
        list::ListItem,
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
    let reaction_emoji =
        replace_emoji_code_with_emoji(&slack_reaction().name.0).unwrap_or("👀".to_string());

    rsx! {
        ListItem {
            key: "{task().id}",
            title: "{task().title}",
            subtitle: rsx! {
                SlackReactionTaskSubtitle { slack_reaction }
                span { class: "tag", "{reaction_emoji}" }
            },
            time: "{task_updated_at}",
            icon: rsx! {
                span { class: "icon-[logos--slack-icon] size-5" }
            },
            meta_icon: rsx! { span { class: "icon-[lucide--hash] w-full h-full" } },
            is_selected,
            on_select,
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
            class: "ui-nrow-meta-text",
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
