#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsPersonCircle, Icon};
use url::Url;

use crate::utils::compute_text_color_from_background_color;

pub mod authentication_tokens_card;
pub mod floating_label_inputs;
pub mod flowbite;
pub mod footer;
pub mod integrations;
pub mod integrations_panel;
pub mod markdown;
pub mod nav_bar;
pub mod notification_preview;
pub mod notifications_list;
pub mod spinner;
pub mod task_link_modal;
pub mod task_planning_modal;
pub mod toast_zone;
pub mod universal_inbox_title;
pub mod user_profile_card;

#[component]
fn CollapseCardWithIcon<'a>(
    cx: Scope,
    icon: Element<'a>,
    title: &'a str,
    children: Element<'a>,
) -> Element {
    render! {
        CollapseCard {
            header: render! { icon, span { "{title}" } },
            children
        }
    }
}

#[component]
fn CollapseCard<'a>(
    cx: Scope,
    header: Element<'a>,
    children: Element<'a>,
    class: Option<&'a str>,
) -> Element {
    let card_style = class.unwrap_or("bg-base-200 text-base-content");

    render! {
        div {
            class: "card w-full {card_style}",

            div {
                class: "card-body p-0",
                div {
                    class: "collapse collapse-arrow",
                    input { "class": "min-h-0 p-2", "type": "checkbox" },
                    div {
                        class: "collapse-title min-h-0 p-2",
                        div { class: "flex items-center gap-2", header }
                    }

                    div { class: "collapse-content", children }
                }
            }
        }
    }
}

#[component]
fn SmallCard<'a>(
    cx: Scope,
    card_class: Option<&'a str>,
    class: Option<&'a str>,
    children: Element<'a>,
) -> Element {
    let card_class = card_class
        .and_then(|card_class| (!card_class.is_empty()).then_some(card_class))
        .unwrap_or("bg-base-200 text-base-content");

    render! {
        div {
            class: "card w-full {card_class}",
            div {
                class: "card-body p-2",
                div {
                    class: "flex items-center gap-2 {class.unwrap_or_default()}",
                    children
                }
            }
        }
    }
}

fn get_initials_from_name(name: &str) -> String {
    name.split_whitespace()
        .take(2)
        .map(|word| word.chars().next().unwrap_or_default())
        .collect::<String>()
        .to_ascii_uppercase()
}

#[component]
pub fn UserWithAvatar(
    cx: Scope,
    user_name: Option<String>,
    avatar_url: Option<Option<Url>>,
    display_name: Option<bool>,
) -> Element {
    let display_name = display_name.unwrap_or_default();
    let initials = user_name.as_ref().map(|name| get_initials_from_name(name));

    render! {
        div {
            class: "flex gap-2 items-center",

            match avatar_url {
                Some(Some(avatar_url)) => render! {
                    div {
                        class: "avatar",
                        div {
                            class: "w-5 rounded-full",
                            img { src: "{avatar_url}" }
                        }
                    }
                },
                Some(None) => {
                    if let Some(initials) = initials {
                        render! {
                            div {
                                class: "avatar placeholder",
                                div {
                                    class: "w-5 rounded-full bg-accent text-accent-content",
                                    span { class: "text-xs", "{initials}" }
                                }
                            }
                        }
                    } else {
                        render! {
                            div {
                                class: "avatar placeholder",
                                div {
                                    class: "w-5 rounded-full bg-accent text-accent-content",
                                    Icon { class: "h-5 w-5", icon: BsPersonCircle }
                                }
                            }
                        }
                    }
                }
                None => None
            }

            if display_name {
                render! {
                    if let Some(user_name) = user_name {
                        render! {
                            span { class: "text-sm", "{user_name}" }
                        }
                    }
                }
            } else {
                None
            }
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum Tag {
    Default { name: String },
    Colored { name: String, color: String },
    Stylized { name: String, class: String },
}

impl From<String> for Tag {
    fn from(name: String) -> Self {
        Tag::Default { name }
    }
}

impl Tag {
    pub fn get_text_class_color(&self, default: &str) -> String {
        match self {
            Tag::Colored { color, .. } => compute_text_color_from_background_color(color),
            _ => default.to_string(),
        }
    }

    pub fn get_style(&self) -> Option<String> {
        match self {
            Tag::Colored { color, .. } => Some(format!("background-color: #{color}")),
            _ => None,
        }
    }

    pub fn get_class(&self) -> Option<String> {
        match self {
            Tag::Stylized { class, .. } => Some(class.clone()),
            _ => None,
        }
    }

    pub fn get_name(&self) -> String {
        match self {
            Tag::Default { name, .. } => name.clone(),
            Tag::Colored { name, .. } => name.clone(),
            Tag::Stylized { name, .. } => name.clone(),
        }
    }
}

#[component]
pub fn TagsInCard(cx: Scope, tags: Vec<Tag>) -> Element {
    if tags.is_empty() {
        return None;
    }

    render! {
        SmallCard {
            class: "flex-wrap",
            for tag in tags {
                render! { TagDisplay { tag: tag.clone() } }
            }
        }
    }
}

#[component]
pub fn TagDisplay<'a>(cx: Scope, tag: Tag, class: Option<&'a str>) -> Element {
    let badge_text_class = tag.get_text_class_color("text-white");
    let badge_class = tag.get_class().unwrap_or_default();
    let badge_style = tag.get_style().unwrap_or_else(|| {
        if badge_class.is_empty() {
            "background-color: #6b7280".to_string()
        } else {
            "".to_string()
        }
    });

    render! {
        div {
            class: "badge {badge_text_class} {badge_class} text-xs text-light {class.unwrap_or_default()} whitespace-nowrap",
            style: "{badge_style}",
            "{tag.get_name()}"
        }
    }
}

#[component]
pub fn CardWithHeaders<'a>(cx: Scope, headers: Vec<Element<'a>>, children: Element<'a>) -> Element {
    render! {
        div {
            class: "card w-full bg-base-200 text-base-content",
            div {
                class: "card-body flex flex-col gap-2 p-2",

                for header in headers {
                    render! {
                        SmallCard {
                            class: "text-xs",
                            card_class: "bg-neutral text-neutral-content",

                            header
                        }
                    }
                }

                children
            }
        }
    }
}
