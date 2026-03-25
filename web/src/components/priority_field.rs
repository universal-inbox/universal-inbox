#![allow(non_snake_case)]

use dioxus::prelude::*;

use crate::theme::{
    PRIORITY_HIGH_COLOR_CLASS, PRIORITY_LOW_COLOR_CLASS, PRIORITY_NORMAL_COLOR_CLASS,
    PRIORITY_URGENT_COLOR_CLASS,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PriorityLevel {
    Urgent,
    High,
    Normal,
    Low,
}

/// Tailwind text-color class for a given priority level. Same palette used by
/// the flag icon in [`PriorityField`] — call this from anywhere else that
/// needs to colorize a glyph by priority (e.g. the task completion icon in
/// list items and preview headers) so the visual cues stay aligned.
pub fn priority_color_class(level: PriorityLevel) -> &'static str {
    match level {
        PriorityLevel::Urgent => PRIORITY_URGENT_COLOR_CLASS,
        PriorityLevel::High => PRIORITY_HIGH_COLOR_CLASS,
        PriorityLevel::Normal => PRIORITY_NORMAL_COLOR_CLASS,
        PriorityLevel::Low => PRIORITY_LOW_COLOR_CLASS,
    }
}

#[component]
pub fn PriorityField(label: String, level: PriorityLevel) -> Element {
    let color_class = priority_color_class(level);

    rsx! {
        span {
            class: "text-[11px] font-bold uppercase tracking-[0.04em] text-ui-base-muted self-center",
            "Priority"
        }
        span {
            class: "text-[12.5px] text-ui-base-content flex items-center gap-1.5 flex-wrap",
            span { class: "icon-[lucide--flag] size-4 {color_class}" }
            span { "{label}" }
        }
    }
}
