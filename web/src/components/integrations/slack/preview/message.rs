#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{Icon, icons::bs_icons::BsArrowUpRightSquare};

use universal_inbox::third_party::integrations::slack::SlackMessageDetails;

use crate::components::{
    CardWithHeaders, MessageHeader,
    integrations::slack::{
        SlackTeamDisplay, get_sender_name_and_avatar, preview::reactions::SlackReactions,
    },
    markdown::SlackMarkdown,
};

#[component]
pub fn SlackMessagePreview(
    slack_message: ReadOnlySignal<SlackMessageDetails>,
    title: ReadOnlySignal<String>,
    icon: Option<Element>,
) -> Element {
    let channel_name = slack_message()
        .channel
        .name
        .clone()
        .unwrap_or_else(|| slack_message().channel.id.to_string());

    rsx! {
        div {
            class: "flex flex-col w-full gap-2 h-full",

            h3 {
                class: "flex items-center gap-2 text-base",

                { icon }
                a {
                    class: "flex items-center",
                    href: "{slack_message().url}",
                    target: "_blank",
                    SlackMarkdown { text: "{title}" }
                    Icon { class: "h-5 w-5 min-w-5 text-base-content/50 p-1", icon: BsArrowUpRightSquare }
                }
            }

            div {
                class: "flex items-center gap-2",

                SlackTeamDisplay { team: slack_message().team }
                a {
                    class: "text-xs text-base-content/50",
                    href: "{slack_message().get_channel_html_url()}",
                    target: "_blank",
                    "#{channel_name}"
                }
            }

            SlackMessageDisplay { slack_message }
        }
    }
}

#[component]
fn SlackMessageDisplay(slack_message: ReadOnlySignal<SlackMessageDetails>) -> Element {
    let posted_at = slack_message().message.origin.ts.to_date_time_opt();
    let text = slack_message().render_content();
    let (user_name, avatar_url) = get_sender_name_and_avatar(&slack_message().sender);

    rsx! {
        div {
            id: "task-preview-details",
            class: "flex flex-col h-full overflow-y-auto scroll-y-auto",
            CardWithHeaders {
                card_class: "bg-neutral text-neutral-content text-xs",
                headers: vec![
                    rsx! {
                        MessageHeader {
                            user_name,
                            avatar_url,
                            display_name: true,
                            sent_at: posted_at
                        }
                    }
                ],

                div {
                    class: "flex flex-col",
                    SlackMarkdown { class: "prose prose-sm", text }

                    if let Some(reactions) = slack_message().message.content.reactions {
                        SlackReactions {
                            reactions,
                            slack_references: slack_message().references.unwrap_or_default(),
                        }
                    }
                }
            }
        }
    }
}
