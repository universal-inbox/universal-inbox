#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsArrowUpRightSquare, Icon};

use universal_inbox::{third_party::integrations::slack::SlackChannelDetails, HasHtmlUrl};

use crate::components::{integrations::slack::SlackTeamDisplay, markdown::SlackMarkdown};

#[component]
pub fn SlackChannelPreview(
    slack_channel: ReadOnlySignal<SlackChannelDetails>,
    title: ReadOnlySignal<String>,
    icon: Option<Element>,
) -> Element {
    let channel_name = slack_channel()
        .channel
        .name
        .clone()
        .unwrap_or_else(|| slack_channel().channel.id.to_string());

    rsx! {
        div {
            class: "flex flex-col w-full gap-2 h-full",

            div {
                class: "flex items-center gap-2",

                SlackTeamDisplay { team: slack_channel().team }
                a {
                    class: "text-xs text-base-content/50",
                    href: "{slack_channel().get_html_url()}",
                    target: "_blank",
                    "#{channel_name}"
                }
            }

            h3 {
                class: "flex items-center gap-2 text-base",

                { icon }
                a {
                    class: "flex items-center",
                    href: "{slack_channel().get_html_url()}",
                    target: "_blank",
                    SlackMarkdown { text: "{title}" }
                    Icon { class: "h-5 w-5 min-w-5 text-base-content/50 p-1", icon: BsArrowUpRightSquare }
                }
            }
        }
    }
}
