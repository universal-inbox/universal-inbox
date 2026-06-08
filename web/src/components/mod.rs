#![allow(non_snake_case)]

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use url::Url;

use universal_inbox::third_party::integrations::todoist::TodoistLabel;

use crate::utils::format_elapsed_time;

pub mod ai_agents_card;
pub mod auth_methods_card;
pub mod auth_widgets;
pub mod authentication_tokens_card;
pub mod datepicker;
pub mod delete_all_confirmation_modal;
pub mod emoji_search_field;
pub mod field_grid;
pub mod floating_label_inputs;
pub mod flyonui;
pub mod footer;
pub mod integrations;
pub mod integrations_panel;
pub mod list;
pub mod loading;
pub mod markdown;
pub mod notification_preview;
pub mod notifications_list;
pub mod oauth_clients_card;
pub mod preview_card_header;
pub mod priority_field;
pub mod project_search_field;
pub mod resizable_panel;
pub mod settings_controls;
pub mod sidebar;
pub mod spinner;
pub mod task_link_modal;
pub mod task_manager_picker;
pub mod task_planning_modal;
pub mod task_preview;
pub mod tasks_list;
pub mod thread;
pub mod threaded_message;
pub mod toast_zone;
pub mod ui;
pub mod universal_inbox_title;
pub mod user_profile_card;
pub mod welcome_hero;

pub fn get_initials_from_name(name: &str) -> String {
    name.split_whitespace()
        .take(2)
        .map(|word| word.chars().next().unwrap_or_default())
        .collect::<String>()
        .to_ascii_uppercase()
}

/// FNV-1a 32-bit hash of the name modulo 12, used to pick a stable avatar hue
/// from the `--ui-avatar-hue-{0..11}` palette in `universal-inbox.css`.
pub fn avatar_hue_index(name: &str) -> u8 {
    let mut hash: u32 = 0x811c9dc5;
    for byte in name.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    (hash % 12) as u8
}

#[component]
pub fn UserWithAvatar(
    user_name: Option<String>,
    avatar_url: Option<Option<Url>>,
    display_name: Option<bool>,
    class: Option<String>,
) -> Element {
    let display_name = display_name.unwrap_or_default();
    let initials = user_name.as_ref().map(|name| get_initials_from_name(name));
    let hue_index = user_name.as_deref().map(avatar_hue_index).unwrap_or(11);
    let avatar_style = format!("background-color: var(--ui-avatar-hue-{hue_index}); color: white;");
    let class = class.unwrap_or("text-sm".to_string());

    rsx! {
        div {
            class: "flex gap-2 items-center {class}",

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
                                class: "avatar avatar-placeholder",
                                div {
                                    class: "w-5 rounded-full",
                                    style: "{avatar_style}",
                                    span { class: "text-xs", "{initials}" }
                                }
                            }
                        }
                    } else {
                        rsx! {
                            div {
                                class: "avatar avatar-placeholder",
                                div {
                                    class: "w-5 rounded-full",
                                    style: "{avatar_style}",
                                    span { class: "icon-[lucide--user-circle] size-5" }
                                }
                            }
                        }
                    }
                }
                None => rsx! {}
            }

            if display_name {
                if let Some(user_name) = user_name {
                    span { "{user_name}" }
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

impl From<TodoistLabel> for Tag {
    fn from(label: TodoistLabel) -> Self {
        Tag::Colored {
            name: label.name,
            color: label.color.to_hex().to_string(),
        }
    }
}

impl Tag {
    pub fn get_name(&self) -> String {
        match self {
            Tag::Default { name, .. } => name.clone(),
            Tag::Colored { name, .. } => name.clone(),
            Tag::Stylized { name, .. } => name.clone(),
        }
    }
}

/// Inline row of label chips. Renders tags as a flex-wrap row with no
/// surrounding card or background — chips alone provide enough visual
/// structure when stacked.
#[component]
pub fn TagList(tags: Vec<Tag>) -> Element {
    if tags.is_empty() {
        return rsx! {};
    }

    rsx! {
        div {
            class: "flex flex-wrap items-center gap-1.5",
            for tag in tags {
                TagDisplay { tag: tag.clone() }
            }
        }
    }
}

#[component]
pub fn TagDisplay(tag: Tag, class: Option<String>) -> Element {
    let extra_class = class.unwrap_or_default();

    match tag {
        // Brand-colored labels render as a muted pill with a colored dot —
        // matches the Todoist label treatment so multiple chips don't compete
        // visually with each other or with surrounding UI chrome.
        Tag::Colored { name, color } => {
            let dot_style = format!("background-color: #{color};");
            rsx! {
                span {
                    class: "inline-flex items-center gap-[5px] pt-px pr-2 pb-px pl-1.5 rounded-ui-pill text-[11px] font-medium bg-ui-base-200 text-ui-base-content border border-ui-border-light whitespace-nowrap {extra_class}",
                    span {
                        class: "w-1.5 h-1.5 rounded-full shrink-0 inline-block",
                        style: "{dot_style}",
                    }
                    "{name}"
                }
            }
        }
        // Semantic chips (warning / info / success / error / muted) keep the
        // class supplied by the caller so they render with their semantic tint.
        Tag::Stylized {
            name,
            class: tag_class,
        } => rsx! {
            span {
                class: "{tag_class} whitespace-nowrap {extra_class}",
                "{name}"
            }
        },
        // Plain labels render as a muted neutral pill (no dot).
        Tag::Default { name } => rsx! {
            span {
                class: "inline-flex items-center gap-[5px] pt-px pr-2 pb-px pl-1.5 rounded-ui-pill text-[11px] font-medium bg-ui-base-200 text-ui-base-content border border-ui-border-light whitespace-nowrap {extra_class}",
                "{name}"
            }
        },
    }
}

#[component]
pub fn MessageHeader(
    user_name: Option<String>,
    avatar_url: Option<Option<Url>>,
    display_name: Option<bool>,
    sent_at: ReadSignal<Option<DateTime<Utc>>>,
    date_class: Option<String>,
) -> Element {
    let sent_at = use_memo(move || sent_at().map(format_elapsed_time));
    let date_class = date_class.unwrap_or_else(|| "text-neutral-content/75".to_string());

    rsx! {
        div {
            class: "flex items-center gap-2 text-xs",

            UserWithAvatar { user_name, avatar_url, display_name }
            if let Some(sent_at) = sent_at() {
                span { class: "{date_class}", "{sent_at}" }
            }
        }
    }
}
