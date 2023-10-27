#![allow(non_snake_case)]

use dioxus::prelude::*;

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
                                div {
                                    class: "flex gap-2 items-center",
                                    icon
                                    "{title}"
                                }
                            }

                            div { class: "collapse-content", children }
                        }
                    }
                } else {
                    render! {
                        div {
                            class: "flex gap-2 items-center p-2",
                            icon
                            "{title}"
                        }
                    }
                }
            }

        }
    }
}
