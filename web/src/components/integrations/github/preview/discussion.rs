#![allow(non_snake_case)]

use dioxus::prelude::*;

use dioxus_free_icons::{icons::bs_icons::BsArrowUpRightSquare, Icon};
use universal_inbox::notification::integrations::github::GithubDiscussion;

use crate::components::{
    integrations::github::{icons::GithubDiscussionIcon, GithubActorDisplay},
    CollapseCard, SmallCard, TagsInCard,
};

#[inline_props]
pub fn GithubDiscussionPreview<'a>(cx: Scope, github_discussion: &'a GithubDiscussion) -> Element {
    render! {
        div {
            class: "flex flex-col w-full gap-2",

            div {
                class: "flex gap-2",

                a {
                    class: "text-xs text-gray-400",
                    href: "{github_discussion.repository.url}",
                    target: "_blank",
                    "{github_discussion.repository.name_with_owner}"
                }

                a {
                    class: "text-xs text-gray-400",
                    href: "{github_discussion.url}",
                    target: "_blank",
                    "#{github_discussion.number}"
                }
            }

            h2 {
                class: "flex items-center gap-2 text-lg",

                GithubDiscussionIcon { class: "h-5 w-5", github_discussion: github_discussion }
                a {
                    href: "{github_discussion.url}",
                    target: "_blank",
                    dangerous_inner_html: "{github_discussion.title}"
                }
                a {
                    class: "flex-none",
                    href: "{github_discussion.url}",
                    target: "_blank",
                    Icon { class: "h-5 w-5 text-gray-400 p-1", icon: BsArrowUpRightSquare }
                }
            }

            GithubDiscussionDetails { github_discussion: github_discussion }
        }
    }
}

#[inline_props]
fn GithubDiscussionDetails<'a>(cx: Scope, github_discussion: &'a GithubDiscussion) -> Element {
    render! {
        div {
            class: "flex flex-col gap-2 w-full",

            div {
                class: "flex text-gray-400 gap-1 text-xs",

                "Created at ",
                span { class: "text-primary", "{github_discussion.created_at}" }
            }

            TagsInCard {
                tags: github_discussion
                    .labels
                    .iter()
                    .map(|label| label.clone().into())
                    .collect()
            }

            if let Some(actor) = &github_discussion.author {
                render! {
                    SmallCard {
                        span { class: "text-gray-400", "Opened by" }
                        GithubActorDisplay { actor: actor }
                    }
                }
            }

            if let Some(actor) = &github_discussion.answer_chosen_by {
                if let Some(answer) = &github_discussion.answer {
                    render! {
                        CollapseCard {
                            header: render! {
                                span { class: "text-gray-400", "Answered by" }
                                GithubActorDisplay { actor: actor }
                            },

                            p {
                                class: "w-full prose prose-sm dark:prose-invert",
                                dangerous_inner_html: "{answer.body}"
                            }
                        }
                    }
                } else {
                    None
                }
            }

            p {
                class: "w-full prose prose-sm dark:prose-invert",
                dangerous_inner_html: "{github_discussion.body}"
            }
        }
    }
}
