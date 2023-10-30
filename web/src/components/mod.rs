#![allow(non_snake_case)]

use dioxus::prelude::*;
use http::Uri;

use crate::utils::compute_text_color_from_background_color;

pub mod floating_label_inputs;
pub mod footer;
pub mod icons;
pub mod integrations;
pub mod integrations_panel;
pub mod nav_bar;
pub mod notification_preview;
pub mod notifications_list;
pub mod spinner;
pub mod task_link_modal;
pub mod task_planning_modal;
pub mod toast_zone;

#[inline_props]
fn CollapseCardWithIcon<'a>(
    cx: Scope,
    icon: Element<'a>,
    title: &'a str,
    children: Element<'a>,
) -> Element {
    render! {
        if children.is_some() {
            render! {
                CollapseCard {
                    header: render! {
                        div { class: "flex gap-2 items-center", icon, "{title}" }
                    },
                    children
                }
            }
        } else {
            render! {
                CollapseCard {
                    header: render! {
                        div { class: "flex gap-2 items-center", icon, "{title}" }
                    }
                }
            }
        }
    }
}

#[inline_props]
fn CollapseCard<'a>(cx: Scope, header: Element<'a>, children: Element<'a>) -> Element {
    render! {
        div {
            class: "card w-full bg-base-200 text-base-content",

            div {
                class: "card-body p-0",
                if children.is_some() {
                    render! {
                        details {
                            class: "collapse collapse-arrow",
                            summary {
                                class: "collapse-title min-h-min p-2",
                                header
                            }

                            div { class: "collapse-content", children }
                        }
                    }
                } else {
                    render! { div { class: "p-2", header } }
                }
            }

        }
    }
}

#[inline_props]
pub fn UserWithAvatar(cx: Scope, user_name: String, avatar_url: Option<Option<Uri>>) -> Element {
    render! {
        div {
            class: "flex gap-2 items-center",

            if let Some(Some(avatar_url)) = avatar_url {
                render! { img { class: "h-5 w-5 rounded-full", src: "{avatar_url}" } }
            }

            span { class: "text-sm", "{user_name}" }
        }
    }
}

#[derive(PartialEq)]
pub struct Tag {
    pub name: String,
    pub color: Option<String>,
}

impl From<String> for Tag {
    fn from(name: String) -> Self {
        Tag { name, color: None }
    }
}

#[inline_props]
pub fn TagsInCard(cx: Scope, tags: Vec<Tag>) -> Element {
    if tags.is_empty() {
        return None;
    }

    render! {
        CollapseCard {
            header: render! {
                div {
                    class: "flex flex-wrap items-center gap-2",
                    for tag in &tags {
                        render! { Tag { tag: tag } }
                    }
                }
            }
        }
    }
}

#[inline_props]
pub fn Tag<'a>(cx: Scope, tag: &'a Tag) -> Element {
    let badge_class = tag
        .color
        .as_ref()
        .map(|color| compute_text_color_from_background_color(color))
        .unwrap_or_else(|| "text-white".to_string());
    let badge_style = tag
        .color
        .as_ref()
        .map(|color| format!("background-color: #{color}"))
        .unwrap_or_else(|| "background-color: #6b7280".to_string());

    render! {
        div {
            class: "badge {badge_class}",
            style: "{badge_style}",
            "{tag.name}"
        }
    }
}
