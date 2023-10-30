#![allow(non_snake_case)]

use dioxus::prelude::*;
use http::Uri;

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
