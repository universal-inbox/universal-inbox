#![allow(non_snake_case)]

use std::fmt;

use dioxus::prelude::dioxus_core::use_drop;
use dioxus::prelude::*;
use dioxus::web::WebEventExt;

use crate::services::flyonui::{forget_flyonui_tooltip_element, init_flyonui_tooltip_element};

#[component]
pub fn Tooltip(
    text: ReadSignal<String>,
    class: ReadSignal<Option<String>>,
    tooltip_class: ReadSignal<Option<String>>,
    placement: Option<TooltipPlacement>,
    disabled: ReadSignal<Option<bool>>,
    children: Element,
) -> Element {
    let placement_class = placement.unwrap_or(TooltipPlacement::Left).to_string();
    let mut mounted_element: Signal<Option<web_sys::Element>> = use_signal(|| None);
    use_drop(move || {
        if let Some(element) = mounted_element() {
            forget_flyonui_tooltip_element(&element);
        }
    });

    if disabled().unwrap_or_default() {
        return rsx! { { children } };
    }

    rsx! {
        div {
            class: "tooltip {placement_class} {class().unwrap_or_default()}",
            onmounted: move |element| {
                let web_element = element.as_web_event();
                init_flyonui_tooltip_element(&web_element);
                mounted_element.set(Some(web_element));
            },

            { children }

            span {
                class: "tooltip-content tooltip-shown:opacity-100 tooltip-shown:visible",
                role: "tooltip",
                span { class: "tooltip-body text-xs {tooltip_class().unwrap_or_default()}", "{text}" }
            }
        }
    }
}

#[allow(dead_code)]
#[derive(PartialEq, Clone)]
pub enum TooltipPlacement {
    Top,
    TopStart,
    TopEnd,
    Bottom,
    BottomStart,
    BottomEnd,
    Left,
    LeftStart,
    LeftEnd,
    Right,
    RightStart,
    RightEnd,
}

impl fmt::Display for TooltipPlacement {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let placement = match self {
            TooltipPlacement::Top => "[--placement:top]",
            TooltipPlacement::TopStart => "[--placement:top-start]",
            TooltipPlacement::TopEnd => "[--placement:top-end]",
            TooltipPlacement::Bottom => "[--placement:bottom]",
            TooltipPlacement::BottomStart => "[--placement:bottom-start]",
            TooltipPlacement::BottomEnd => "[--placement:bottom-end]",
            TooltipPlacement::Left => "[--placement:left]",
            TooltipPlacement::LeftStart => "[--placement:left-start]",
            TooltipPlacement::LeftEnd => "[--placement:left-end]",
            TooltipPlacement::Right => "[--placement:right]",
            TooltipPlacement::RightStart => "[--placement:right-start]",
            TooltipPlacement::RightEnd => "[--placement:right-end]",
        };
        write!(f, "{}", placement)
    }
}
