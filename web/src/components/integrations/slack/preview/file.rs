#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::third_party::integrations::slack::SlackFileDetails;

use crate::{
    components::{integrations::slack::SlackTeamDisplay, preview_card_header::PreviewCardHeader},
    utils::strip_markdown_links,
};

#[component]
pub fn SlackFilePreview(
    slack_file: ReadSignal<SlackFileDetails>,
    title: ReadSignal<String>,
) -> Element {
    let channel_name = slack_file()
        .channel
        .name
        .clone()
        .unwrap_or_else(|| slack_file().channel.id.to_string());

    rsx! {
        div {
            class: "flex flex-col w-full h-full",

            PreviewCardHeader {
                brand_icon: rsx! { span { class: "icon-[lucide--paperclip] size-4" } },
                title: strip_markdown_links(&title()),
                subline: rsx! {
                    SlackTeamDisplay { team: slack_file().team, display_name: true }
                    span { class: "sep", "·" }
                    span { "#{channel_name}" }
                },
            }
        }
    }
}
