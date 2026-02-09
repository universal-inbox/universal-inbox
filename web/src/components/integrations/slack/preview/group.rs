#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{Icon, icons::bs_icons::BsArrowUpRightSquare};

use universal_inbox::{HasHtmlUrl, third_party::integrations::slack::SlackGroupDetails};

use crate::components::{integrations::slack::SlackTeamDisplay, markdown::SlackMarkdown};

#[component]
pub fn SlackGroupPreview(
    slack_group: ReadSignal<SlackGroupDetails>,
    title: ReadSignal<String>,
    icon: Option<Element>,
) -> Element {
    let channel_name = slack_group()
        .channel
        .name
        .clone()
        .unwrap_or_else(|| slack_group().channel.id.to_string());

    rsx! {
        div {
            class: "flex flex-col w-full gap-2 h-full",

            h3 {
                class: "flex items-center gap-2 text-base",

                { icon }
                a {
                    class: "flex items-center",
                    href: "{slack_group().get_html_url()}",
                    target: "_blank",
                    SlackMarkdown { text: "{title}" }
                    Icon { class: "h-5 w-5 min-w-5 text-base-content/50 p-1", icon: BsArrowUpRightSquare }
                }
            }

            div {
                class: "flex items-center gap-2",

                SlackTeamDisplay { team: slack_group().team }
                a {
                    class: "text-xs text-base-content/50",
                    href: "{slack_group().get_html_url()}",
                    target: "_blank",
                    "#{channel_name}"
                }
            }
        }
    }
}
