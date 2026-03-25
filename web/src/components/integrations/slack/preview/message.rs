#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::third_party::integrations::slack::SlackMessageDetails;

use crate::{
    components::{
        MessageHeader,
        integrations::slack::{
            SlackTeamDisplay, get_sender_name_and_avatar, preview::reactions::SlackReactions,
        },
        markdown::SlackHtml,
        preview_card_header::PreviewCardHeader,
        ui::{Card, CardVariant},
    },
    utils::strip_markdown_links,
};

#[component]
pub fn SlackMessagePreview(
    slack_message: ReadSignal<SlackMessageDetails>,
    title: ReadSignal<String>,
) -> Element {
    let channel_name = slack_message()
        .channel
        .name
        .clone()
        .unwrap_or_else(|| slack_message().channel.id.to_string());

    rsx! {
        div {
            class: "flex flex-col w-full h-full",

            PreviewCardHeader {
                brand_icon: rsx! { span { class: "icon-[lucide--message-square-text] size-4" } },
                title: strip_markdown_links(&title()),
                subline: rsx! {
                    SlackTeamDisplay { team: slack_message().team, display_name: true, class: "" }
                    span { class: "sep", "·" }
                    span { "#{channel_name}" }
                },
            }

            SlackMessageDisplay { slack_message }
        }
    }
}

#[component]
fn SlackMessageDisplay(slack_message: ReadSignal<SlackMessageDetails>) -> Element {
    let posted_at = slack_message().message.origin.ts.to_date_time_opt();
    let html = slack_message().render_content_as_html("text-primary", "text-warning", None);
    let (user_name, avatar_url) = get_sender_name_and_avatar(&slack_message().sender);

    rsx! {
        div {
            id: "task-preview-details",
            class: "flex flex-col gap-2 w-full h-full overflow-y-auto scroll-y-auto p-3",
            Card {
                variant: CardVariant::Default,
                div {
                    class: "flex items-center gap-2 text-xs",
                    style: "color: var(--ui-base-muted); margin-bottom: 8px;",
                    MessageHeader {
                        user_name,
                        avatar_url,
                        display_name: true,
                        sent_at: posted_at
                    }
                }
                div {
                    class: "flex flex-col",
                    SlackHtml { class: "prose prose-sm", html }

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
