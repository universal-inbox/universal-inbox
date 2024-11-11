#![allow(non_snake_case)]

use dioxus::prelude::*;

use dioxus_free_icons::{icons::bs_icons::BsArrowUpRightSquare, Icon};
use universal_inbox::third_party::integrations::github::GithubDiscussion;

use crate::components::{
    integrations::github::{icons::GithubDiscussionIcon, GithubActorDisplay},
    CollapseCard, SmallCard, TagsInCard,
};

#[component]
pub fn GithubDiscussionPreview(github_discussion: ReadOnlySignal<GithubDiscussion>) -> Element {
    rsx! {
        div {
            class: "flex flex-col w-full gap-2",

            div {
                class: "flex gap-2",

                a {
                    class: "text-xs text-gray-400",
                    href: "{github_discussion().repository.url}",
                    target: "_blank",
                    "{github_discussion().repository.name_with_owner}"
                }

                a {
                    class: "text-xs text-gray-400",
                    href: "{github_discussion().url}",
                    target: "_blank",
                    "#{github_discussion().number}"
                }
            }

            h2 {
                class: "flex items-center gap-2 text-lg",

                GithubDiscussionIcon { class: "h-5 w-5", github_discussion: github_discussion() }
                a {
                    href: "{github_discussion().url}",
                    target: "_blank",
                    dangerous_inner_html: "{github_discussion().title}"
                }
                a {
                    class: "flex-none",
                    href: "{github_discussion().url}",
                    target: "_blank",
                    Icon { class: "h-5 w-5 text-gray-400 p-1", icon: BsArrowUpRightSquare }
                }
            }

            GithubDiscussionDetails { github_discussion: github_discussion }
        }
    }
}

#[component]
fn GithubDiscussionDetails(github_discussion: ReadOnlySignal<GithubDiscussion>) -> Element {
    rsx! {
        div {
            class: "flex flex-col gap-2 w-full",

            div {
                class: "flex text-gray-400 gap-1 text-xs",

                "Created at ",
                span { class: "text-primary", "{github_discussion().created_at}" }
            }

            TagsInCard {
                tags: github_discussion()
                    .labels
                    .iter()
                    .map(|label| label.clone().into())
                    .collect()
            }

            if let Some(actor) = &github_discussion().author {
                SmallCard {
                    span { class: "text-gray-400", "Opened by" }
                    GithubActorDisplay { actor: actor.clone(), display_name: true }
                }
            }

            if let Some(actor) = &github_discussion().answer_chosen_by {
                if let Some(answer) = &github_discussion().answer {
                    CollapseCard {
                        header: rsx! {
                            span { class: "text-gray-400", "Answered by" }
                            GithubActorDisplay { actor: actor.clone(), display_name: true }
                        },

                        p {
                            class: "w-full prose prose-sm dark:prose-invert",
                            dangerous_inner_html: "{answer.body}"
                        }
                    }
                }
            }

            p {
                class: "w-full prose prose-sm dark:prose-invert",
                dangerous_inner_html: "{github_discussion().body}"
            }
        }
    }
}
