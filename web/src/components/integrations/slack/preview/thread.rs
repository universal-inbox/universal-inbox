#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsArrowUpRightSquare, Icon};
use slack_morphism::{SlackHistoryMessage, SlackMessageSender};

use universal_inbox::third_party::integrations::slack::{SlackMessageRender, SlackThread};

use crate::components::{
    integrations::slack::{SlackMessageActorDisplay, SlackTeamDisplay},
    markdown::Markdown,
};

#[component]
pub fn SlackThreadPreview(
    slack_thread: ReadOnlySignal<SlackThread>,
    title: ReadOnlySignal<String>,
) -> Element {
    let channel_name = slack_thread()
        .channel
        .name
        .clone()
        .unwrap_or_else(|| slack_thread().channel.id.to_string());

    rsx! {
        div {
            class: "flex flex-col w-full gap-2",

            div {
                class: "flex items-center gap-2",

                SlackTeamDisplay { team: slack_thread().team }

                a {
                    class: "text-xs text-gray-400",
                    href: "{slack_thread().get_channel_html_url()}",
                    target: "_blank",
                    "#{channel_name}"
                }

                a {
                    class: "flex-none",
                    href: "{slack_thread().url}",
                    target: "_blank",
                    Icon { class: "h-5 w-5 text-gray-400 p-1", icon: BsArrowUpRightSquare }
                }
            }

            SlackThreadDisplay { slack_thread }
        }
    }
}

#[component]
fn SlackThreadDisplay(slack_thread: ReadOnlySignal<SlackThread>) -> Element {
    let mut show_all = use_signal(|| false);
    let messages = slack_thread().messages;
    let last_read_message_index = if let Some(last_read) = slack_thread().last_read {
        messages
            .iter()
            .position(|message| message.origin.ts == last_read)
            .map(|index| index + 1)
            .unwrap_or(0)
    } else {
        0
    };
    let (read_messages, unread_messages) = messages.split_at(last_read_message_index);
    let read_messages_to_display: Option<(
        SlackHistoryMessage,
        SlackHistoryMessage,
        Option<String>,
    )> = if let Some((first, rest)) = read_messages.split_first() {
        if let Some((last, rest)) = rest.split_last() {
            let length = rest.len();
            let invisible_read_message = match length {
                0 => None,
                1 => Some("1 hidden reply...".to_string()),
                n => Some(format!("{n} hidden replies...")),
            };
            Some((first.clone(), last.clone(), invisible_read_message))
        } else {
            None
        }
    } else {
        None
    };
    let unread_message = match unread_messages.len() {
        0 => None,
        1 => Some("1 unread reply".to_string()),
        n => Some(format!("{n} unread replies")),
    };

    rsx! {
        div {
            class: "card w-full bg-base-200 text-base-content",
            div {
                class: "card-body p-2 flex flex-col gap-2",

                if !show_all() {
                    if let Some((first_read_message, last_read_message, invisible_read_message)) = read_messages_to_display {
                        SlackThreadMessageDisplay { message: first_read_message, slack_thread }
                        if let Some(invisible_read_message) = invisible_read_message {
                            div {
                                class: "flex items-center gap-2 text-xs text-gray-400",
                                a {
                                    class: "link link-hover link-primary",
                                    onclick: move |_| { *show_all.write() = true; },
                                    "{invisible_read_message}"
                                },
                            }
                        }
                        SlackThreadMessageDisplay { message: last_read_message, slack_thread }
                    } else {
                        for message in read_messages {
                            SlackThreadMessageDisplay { message: message.clone(), slack_thread }
                        }
                    }
                } else {
                    for message in read_messages {
                        SlackThreadMessageDisplay { message: message.clone(), slack_thread }
                    }
                }

                if let Some(unread_message) = unread_message {
                    div {
                        class: "divider divider-primary grow my-0 text-xs text-primary",
                        "{unread_message}"
                    }
                    for message in unread_messages {
                        SlackThreadMessageDisplay { message: message.clone(), slack_thread }
                    }
                }
            }
        }
    }
}

#[component]
fn SlackThreadMessageDisplay(
    message: ReadOnlySignal<SlackHistoryMessage>,
    slack_thread: ReadOnlySignal<SlackThread>,
) -> Element {
    let posted_at = message().origin.ts.to_date_time_opt();
    let sender_id = match message().sender {
        SlackMessageSender {
            user: Some(ref user_id),
            ..
        } => Some(user_id.to_string()),
        SlackMessageSender {
            bot_id: Some(ref bot_id),
            ..
        } => Some(bot_id.to_string()),
        _ => None,
    };
    let sender = sender_id.and_then(|id| slack_thread().sender_profiles.get(&id).cloned());
    let text = message().render_content(slack_thread().references.clone(), true);

    rsx! {
        div {
            class: "flex flex-col gap-0",
            div {
                class: "flex items-center gap-2 text-xs text-gray-400",
                if let Some(sender) = sender {
                    SlackMessageActorDisplay { sender, display_name: true }
                }
                if let Some(posted_at) = posted_at {
                    span { "{posted_at}" }
                }
            }

            Markdown { text }
        }
    }
}
