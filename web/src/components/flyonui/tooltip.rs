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
    /// Make the wrapper spans `block w-full` so a block-level child (e.g. a
    /// `SettingRow` using `justify-between`) keeps its full width instead of
    /// collapsing to content width. Leave `false` (default) when wrapping
    /// inline targets like icons or buttons.
    #[props(default = false)]
    full_width: bool,
    children: Element,
) -> Element {
    if disabled().unwrap_or_default() {
        return rsx! { { children } };
    }

    let placement_attr = placement.unwrap_or(TooltipPlacement::Left).to_data_attr();
    let width_class = if full_width { "block w-full" } else { "" };
    let mut mounted_element: Signal<Option<web_sys::Element>> = use_signal(|| None);

    use_drop(move || {
        if let Some(element) = mounted_element() {
            forget_flyonui_tooltip_element(&element);
        }
    });

    rsx! {
        span {
            class: "tooltip {width_class} {tooltip_class().unwrap_or_default()} {class().unwrap_or_default()}",
            style: "--placement: {placement_attr};",
            onmounted: move |element| {
                let web_element = element.as_web_event();
                init_flyonui_tooltip_element(&web_element);
                mounted_element.set(Some(web_element));
            },

            span {
                class: "tooltip-toggle {width_class}",
                { children }
            }

            span {
                class: "tooltip-content tooltip-shown:opacity-100 tooltip-shown:visible hidden",
                role: "tooltip",
                span { class: "tooltip-body", "{text}" }
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

impl TooltipPlacement {
    pub fn to_data_attr(&self) -> &'static str {
        match self {
            TooltipPlacement::Top => "top",
            TooltipPlacement::TopStart => "top-start",
            TooltipPlacement::TopEnd => "top-end",
            TooltipPlacement::Bottom => "bottom",
            TooltipPlacement::BottomStart => "bottom-start",
            TooltipPlacement::BottomEnd => "bottom-end",
            TooltipPlacement::Left => "left",
            TooltipPlacement::LeftStart => "left-start",
            TooltipPlacement::LeftEnd => "left-end",
            TooltipPlacement::Right => "right",
            TooltipPlacement::RightStart => "right-start",
            TooltipPlacement::RightEnd => "right-end",
        }
    }
}

impl fmt::Display for TooltipPlacement {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_data_attr())
    }
}
