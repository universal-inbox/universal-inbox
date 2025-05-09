#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsArrowUpRightSquare, Icon};
use slack_morphism::SlackHistoryMessage;

use universal_inbox::third_party::integrations::slack::{SlackMessageRender, SlackThread};

use crate::components::{
    integrations::slack::{
        get_sender_name_and_avatar, preview::reactions::SlackReactions, SlackTeamDisplay,
    },
    markdown::SlackMarkdown,
    MessageHeader,
};

#[component]
pub fn SlackThreadPreview(
    slack_thread: ReadOnlySignal<SlackThread>,
    title: ReadOnlySignal<String>,
    expand_details: ReadOnlySignal<bool>,
) -> Element {
    let channel_name = slack_thread()
        .channel
        .name
        .clone()
        .unwrap_or_else(|| slack_thread().channel.id.to_string());

    rsx! {
        div {
            class: "flex flex-col w-full gap-2 h-full",

            div {
                class: "flex items-center gap-2",

                SlackTeamDisplay { team: slack_thread().team }

                a {
                    class: "flex items-center text-xs text-base-content/50",
                    href: "{slack_thread().url}",
                    target: "_blank",
                    "#{channel_name}"
                    Icon { class: "h-5 w-5 min-w-5 text-base-content/50 p-1", icon: BsArrowUpRightSquare }
                }
            }

            SlackThreadDisplay { slack_thread, expand_details }
        }
    }
}

#[component]
fn SlackThreadDisplay(
    slack_thread: ReadOnlySignal<SlackThread>,
    expand_details: ReadOnlySignal<bool>,
) -> Element {
    let mut show_all = use_signal(|| false);
    let _ = use_resource(move || async move {
        *show_all.write() = expand_details();
    });
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
            id: "notification-preview-details",
            class: "card w-full bg-base-200 h-full overflow-y-auto scroll-y-auto",
            div {
                class: "card-body p-2 flex flex-col gap-2",

                if !show_all() {
                    if let Some((first_read_message, last_read_message, invisible_read_message)) = read_messages_to_display {
                        SlackThreadMessageDisplay { message: first_read_message, slack_thread }
                        if let Some(invisible_read_message) = invisible_read_message {
                            div {
                                class: "flex items-center gap-2 text-xs text-base-content/50",
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
                        class: "divider divider-primary my-0 text-xs text-primary",
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
    let sender = message()
        .get_sender(&slack_thread().sender_profiles)
        .map(|sender| get_sender_name_and_avatar(&sender));
    let text = message().render_content(slack_thread().references.clone(), true);

    rsx! {
        div {
            class: "flex flex-col gap-0",
            div {
                class: "flex items-center gap-2 text-xs text-base-content/50",

                if let Some((user_name, avatar_url)) = sender {
                    MessageHeader {
                        user_name,
                        avatar_url,
                        display_name: true,
                        sent_at: posted_at,
                        date_class: "text-base-content/75",
                    }
                }
            }

            div {
                class: "flex flex-col",
                SlackMarkdown { class: "prose prose-sm", text }

                if let Some(reactions) = message().content.reactions {
                    SlackReactions {
                        reactions,
                        slack_references: slack_thread().references.unwrap_or_default(),
                    }
                }
            }
        }
    }
}
