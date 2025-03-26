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
pub mod list;
pub mod loading;
pub mod markdown;
pub mod nav_bar;
pub mod notification_preview;
pub mod notifications_list;
pub mod spinner;
pub mod task_link_modal;
pub mod task_planning_modal;
pub mod task_preview;
pub mod tasks_list;
pub mod toast_zone;
pub mod universal_inbox_title;
pub mod user_profile_card;

#[component]
fn CollapseCardWithIcon(
    icon: Element,
    title: ReadOnlySignal<String>,
    opened: Option<bool>,
    children: Element,
) -> Element {
    rsx! {
        CollapseCard {
            header: rsx! { { icon }, span { "{title}" } },
            opened: opened.unwrap_or_default(),
            children
        }
    }
}

#[component]
fn CollapseCard(
    header: Element,
    children: Element,
    class: Option<String>,
    opened: Option<bool>,
) -> Element {
    let card_style = class.unwrap_or("bg-base-200 text-base-content".to_string());
    let checked = opened.unwrap_or_default();

    rsx! {
        div {
            class: "card w-full {card_style}",

            div {
                class: "card-body p-0",
                div {
                    class: "collapse collapse-arrow",
                    input { "class": "min-h-0! p-2", "type": "checkbox", checked },
                    div {
                        class: "collapse-title min-h-0 p-2",
                        div { class: "flex items-center gap-2", { header } }
                    }

                    div { class: "collapse-content", { children } }
                }
            }
        }
    }
}

#[component]
fn SmallCard(card_class: Option<String>, class: Option<String>, children: Element) -> Element {
    let card_class = card_class
        .and_then(|card_class| (!card_class.is_empty()).then_some(card_class))
        .unwrap_or("bg-base-200 text-base-content".to_string());
    let class = class.unwrap_or_default();

    rsx! {
        div {
            class: "card w-full {card_class}",
            div {
                class: "card-body p-2",
                div {
                    class: "flex items-center gap-2 {class}",
                    { children }
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
    user_name: Option<String>,
    avatar_url: Option<Option<Url>>,
    display_name: Option<bool>,
) -> Element {
    let display_name = display_name.unwrap_or_default();
    let initials = user_name.as_ref().map(|name| get_initials_from_name(name));

    rsx! {
        div {
            class: "flex gap-2 items-center",

            match avatar_url {
                Some(Some(avatar_url)) => rsx! {
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
                        rsx! {
                            div {
                                class: "avatar placeholder",
                                div {
                                    class: "w-5 rounded-full bg-accent text-accent-content",
                                    span { class: "text-xs", "{initials}" }
                                }
                            }
                        }
                    } else {
                        rsx! {
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
                None => rsx! {}
            }

            if display_name {
                if let Some(user_name) = user_name {
                    span { class: "text-sm", "{user_name}" }
                }
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
pub fn TagsInCard(tags: Vec<Tag>) -> Element {
    if tags.is_empty() {
        return rsx! {};
    }

    rsx! {
        SmallCard {
            class: "flex-wrap",
            for tag in tags {
                TagDisplay { tag: tag.clone() }
            }
        }
    }
}

#[component]
pub fn TagDisplay(tag: Tag, class: Option<String>) -> Element {
    let badge_text_class = tag.get_text_class_color("text-white");
    let badge_class = tag.get_class().unwrap_or_default();
    let badge_style = tag.get_style().unwrap_or_else(|| {
        if badge_class.is_empty() {
            "background-color: #6b7280".to_string()
        } else {
            "".to_string()
        }
    });
    let class = class.unwrap_or_default();

    rsx! {
        div {
            class: "badge badge-sm {badge_text_class} {badge_class} text-xs text-light {class} whitespace-nowrap",
            style: "{badge_style}",
            "{tag.get_name()}"
        }
    }
}

#[component]
pub fn CardWithHeaders(headers: Vec<Element>, children: Element) -> Element {
    rsx! {
        div {
            class: "card w-full bg-base-200 text-base-content",
            div {
                class: "card-body flex flex-col gap-2 p-2",

                for header in headers {
                    SmallCard {
                        class: "text-xs",
                        card_class: "bg-neutral text-neutral-content",

                        { header }
                    }
                }

                { children }
            }
        }
    }
}
