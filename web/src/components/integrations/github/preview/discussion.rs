#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::{
    third_party::integrations::github::GithubDiscussion,
    utils::emoji::replace_emoji_code_with_emoji,
};

use crate::{
    components::{
        TagList, UserWithAvatar,
        integrations::github::{GithubActorDisplay, get_github_actor_name_and_url},
        preview_card_header::PreviewCardHeader,
        ui::{Card, CardVariant, MetadataGrid, MetadataItem, Tag as UiTag, TagVariant},
    },
    utils::format_elapsed_time,
};

#[component]
pub fn GithubDiscussionPreview(
    github_discussion: ReadSignal<GithubDiscussion>,
    expand_details: ReadSignal<bool>,
) -> Element {
    let discussion = github_discussion();
    let is_answered = discussion.answer_chosen_at.is_some();

    let (state_variant, state_label) = if is_answered {
        (TagVariant::Success, "Answered")
    } else {
        (TagVariant::Info, "Open")
    };

    let created_ago = format_elapsed_time(discussion.created_at);

    let title = discussion.title.clone();
    let identifier = format!("#{}", discussion.number);
    let repo_name = discussion.repository.name_with_owner.clone();
    let repo_url = discussion.repository.url.clone();
    let discussion_url = discussion.url.clone();
    let author = discussion.author.clone();

    rsx! {
        div {
            class: "flex flex-col w-full h-full",

            PreviewCardHeader {
                brand_icon: rsx! { span { class: "icon-[lucide--message-square] size-4" } },
                title,
                identifier: Some(identifier),
                subline: rsx! {
                    if let Some(actor) = author {
                        span { "Opened by" }
                        {
                            let (name, url) = get_github_actor_name_and_url(actor);
                            rsx! {
                                UserWithAvatar {
                                    user_name: name,
                                    avatar_url: Some(Some(url)),
                                    display_name: true,
                                    class: "text-[11px]",
                                }
                            }
                        }
                        span { class: "sep", "·" }
                        span { "{created_ago} ago" }
                    }
                }
            }

            div {
                id: "notification-preview-details",
                class: "flex flex-col gap-2 w-full h-full overflow-y-auto scroll-y-auto p-3",

                Card {
                    variant: CardVariant::Default,

                    MetadataGrid {
                        MetadataItem {
                            label: "Repository".to_string(),
                            value: rsx! {
                                a {
                                    href: "{repo_url}",
                                    target: "_blank",
                                    rel: "noopener noreferrer",
                                    "{repo_name}"
                                }
                                a {
                                    href: "{discussion_url}",
                                    target: "_blank",
                                    rel: "noopener noreferrer",
                                    "#{discussion.number}"
                                }
                            },
                        }

                        MetadataItem {
                            label: "State".to_string(),
                            value: rsx! {
                                UiTag { variant: state_variant, "{state_label}" }
                            },
                        }

                        if let Some(category) = &discussion.category {
                            MetadataItem {
                                label: "Category".to_string(),
                                value: rsx! {
                                    if let Some(emoji_glyph) = category.emoji.as_deref().and_then(replace_emoji_code_with_emoji) {
                                        span { "{emoji_glyph}" }
                                    }
                                    span { "{category.name}" }
                                },
                            }
                        }

                        MetadataItem {
                            label: "Updated".to_string(),
                            value: rsx! {
                                span { "{format_elapsed_time(discussion.updated_at)} ago" }
                            },
                        }
                    }

                    if !discussion.labels.is_empty() {
                        TagList {
                            tags: discussion
                                .labels
                                .iter()
                                .map(|label| label.clone().into())
                                .collect()
                        }
                    }
                }

                Card {
                    variant: CardVariant::Default,
                    div {
                        class: "w-full max-w-full prose prose-sm dark:prose-invert",
                        dangerous_inner_html: "{discussion.body}"
                    }
                }

                if let (Some(answer), Some(actor)) = (&discussion.answer, &discussion.answer_chosen_by) {
                    div {
                        class: "preview-card",
                        style: "background: var(--ui-success-subtle);",

                        div {
                            class: "flex items-center gap-2 text-xs mb-2",
                            span {
                                class: "icon-[lucide--check-circle] size-4",
                                style: "color: var(--ui-success);",
                            }
                            span {
                                style: "color: var(--ui-success); font-weight: 600;",
                                "Accepted answer by"
                            }
                            GithubActorDisplay { actor: actor.clone(), display_name: true }
                        }
                        div {
                            class: "w-full max-w-full prose prose-sm dark:prose-invert",
                            dangerous_inner_html: "{answer.body}"
                        }
                    }
                }

                if expand_details() { div { class: "hidden" } }
            }
        }
    }
}
