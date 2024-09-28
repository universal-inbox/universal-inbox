#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsArrowUpRightSquare, Icon};

use universal_inbox::{notification::integrations::slack::SlackImDetails, HasHtmlUrl};

use crate::components::integrations::slack::{icons::SlackNotificationIcon, SlackTeamDisplay};

#[component]
pub fn SlackImPreview(
    slack_im: ReadOnlySignal<SlackImDetails>,
    title: ReadOnlySignal<String>,
) -> Element {
    let channel_name = slack_im()
        .channel
        .name
        .clone()
        .unwrap_or_else(|| slack_im().channel.id.to_string());

    rsx! {
        div {
            class: "flex flex-col w-full gap-2",

            div {
                class: "flex items-center gap-2",

                SlackTeamDisplay { team: slack_im().team }
                a {
                    class: "text-xs text-gray-400",
                    href: "{slack_im().get_html_url()}",
                    target: "_blank",
                    "#{channel_name}"
                }
            }

            h2 {
                class: "flex items-center gap-2 text-lg",

                SlackNotificationIcon { class: "h-5 w-5" }
                a {
                    href: "{slack_im().get_html_url()}",
                    target: "_blank",
                    dangerous_inner_html: "{title}"
                }
                a {
                    class: "flex-none",
                    href: "{slack_im().get_html_url()}",
                    target: "_blank",
                    Icon { class: "h-5 w-5 text-gray-400 p-1", icon: BsArrowUpRightSquare }
                }
            }
        }
    }
}
