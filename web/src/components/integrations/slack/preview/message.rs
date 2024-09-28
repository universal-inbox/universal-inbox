#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsArrowUpRightSquare, Icon};
use log::debug;
use regex::Regex;

use universal_inbox::notification::integrations::slack::SlackMessageDetails;

use crate::components::{
    integrations::slack::{
        icons::SlackNotificationIcon, SlackMessageActorDisplay, SlackTeamDisplay,
    },
    markdown::Markdown,
    CardWithHeaders,
};

#[component]
pub fn SlackMessagePreview(
    slack_message: ReadOnlySignal<SlackMessageDetails>,
    title: ReadOnlySignal<String>,
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

                SlackNotificationIcon { class: "h-5 w-5" }
                a {
                    href: "{slack_message().url}",
                    target: "_blank",
                    dangerous_inner_html: "{title}"
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
    let message = slack_message()
        .message
        .content
        .text
        .as_ref()
        .map(|msg| sanitize_slack_markdown(msg.as_str()));

    rsx! {
        CardWithHeaders {
            headers: vec![
                rsx! {
                    div {
                        class: "flex items-center gap-2",
                        SlackMessageActorDisplay { slack_message, display_name: true }
                        if let Some(ref posted_at) = posted_at {
                            span { class: "text-xs text-gray-400", "{posted_at}" }
                        }
                    }
                }
            ],

            if let Some(message) = message {
                Markdown { text: message }
            }
        }
    }
}

fn sanitize_slack_markdown(slack_markdown: &str) -> String {
    // Replace slack markdown with common markdown
    // This could be more robustly implemented using Slack blocks
    let regexs = [
        (Regex::new(r"^```").unwrap(), "```\n"),
        (Regex::new(r"```$").unwrap(), "\n```"),
        (Regex::new(r"^• ").unwrap(), "- "),
        (Regex::new(r"^(\s*)◦ ").unwrap(), "$1- "),
        (Regex::new(r"^&gt; ").unwrap(), "> "),
        (Regex::new(r"<([^|]+)\|([^>]+)>").unwrap(), "[$2]($1)"),
    ];
    let res = slack_markdown
        .lines()
        .map(|line| {
            regexs
                .iter()
                .fold(line.to_string(), |acc, (re, replacement)| {
                    re.replace(&acc, *replacement).to_string()
                })
        })
        .collect::<Vec<String>>()
        .join("\n");
    debug!("Sanitized slack message: {res}");
    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn test_sanitize_slack_markdown_code() {
        assert_eq!(
            sanitize_slack_markdown("```$ echo Hello```"),
            "```\n$ echo Hello\n```"
        );
        assert_eq!(
            sanitize_slack_markdown("test: ```$ echo Hello```."),
            "test: ```$ echo Hello```."
        );
    }

    #[wasm_bindgen_test]
    fn test_sanitize_slack_markdown_list() {
        assert_eq!(sanitize_slack_markdown("• item"), "- item");
        assert_eq!(sanitize_slack_markdown("test: • item"), "test: • item");
    }

    #[wasm_bindgen_test]
    fn test_sanitize_slack_markdown_sublist() {
        assert_eq!(sanitize_slack_markdown(" ◦ subitem"), " - subitem");
        assert_eq!(
            sanitize_slack_markdown("test: ◦ subitem"),
            "test: ◦ subitem"
        );
    }

    #[wasm_bindgen_test]
    fn test_sanitize_slack_markdown_quote() {
        assert_eq!(sanitize_slack_markdown("&gt; "), "> ");
        assert_eq!(sanitize_slack_markdown("test: &gt; "), "test: &gt; ");
    }

    #[wasm_bindgen_test]
    fn test_sanitize_slack_markdown_link() {
        assert_eq!(
            sanitize_slack_markdown("This is a <https://www.example.com|link> to www.example.com"),
            "This is a [link](https://www.example.com) to www.example.com"
        );
    }
}
