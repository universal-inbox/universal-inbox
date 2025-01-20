#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsArrowUpRightSquare, Icon};

use universal_inbox::third_party::integrations::slack::SlackMessageDetails;

use crate::components::{
    integrations::slack::{
        preview::reactions::SlackReactions, SlackMessageActorDisplay, SlackTeamDisplay,
    },
    markdown::SlackMarkdown,
    CardWithHeaders,
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
            class: "flex flex-col w-full gap-2",

            div {
                class: "flex items-center gap-2",

                SlackTeamDisplay { team: slack_message().team }
                a {
                    class: "text-xs text-gray-400",
                    href: "{slack_message().get_channel_html_url()}",
                    target: "_blank",
                    "#{channel_name}"
                }
            }

            h2 {
                class: "flex items-center gap-2 text-lg",

                { icon }
                a {
                    href: "{slack_message().url}",
                    target: "_blank",

                    SlackMarkdown { text: "{title}" }
                }
                a {
                    class: "flex-none",
                    href: "{slack_message().url}",
                    target: "_blank",
                    Icon { class: "h-5 w-5 text-gray-400 p-1", icon: BsArrowUpRightSquare }
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

    rsx! {
        CardWithHeaders {
            headers: vec![
                rsx! {
                    div {
                        class: "flex items-center gap-2",
                        SlackMessageActorDisplay { sender: slack_message().sender, display_name: true }
                        if let Some(ref posted_at) = posted_at {
                            span { class: "text-xs text-gray-400", "{posted_at}" }
                        }
                    }
                }
            ],

            div {
                class: "flex flex-col",
                SlackMarkdown { text }

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
