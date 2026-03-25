//! Keyboard primitives for the Universal Inbox design system.
//!
//! Two Dioxus components — `Kbd` (a single keycap) and `KeyboardHint`
//! (one or more keycaps + a descriptive label) — styled exclusively via
//! Tailwind v4 utilities backed by the project's `@theme` tokens. Shared
//! across sizes: `rounded-ui-xs`, `border-ui-border`, `bg-ui-base-200`,
//! `font-semibold`. Per-size: `font-mono` + `text-ui-base-content` (Sm) and
//! `font-ui` + `text-ui-base-muted` (Xs).
//!
//! ## Usage
//!
//! ```ignore
//! use crate::components::ui::{Kbd, KbdSize, KeyboardHint};
//!
//! // Single keycap (default size)
//! rsx! { Kbd { label: "?".to_string() } }
//!
//! // Compact keycap, e.g. inside the footer keyboard hints row
//! rsx! { Kbd { label: "↑".to_string(), size: KbdSize::Xs } }
//!
//! // Full keyboard hint (one or more keys + descriptive label)
//! rsx! {
//!     KeyboardHint {
//!         keys: vec!["↑".to_string(), "↓".to_string()],
//!         label: "navigate".to_string(),
//!     }
//! }
//! ```
//!
//! Both components render semantic HTML — `Kbd` emits a real `<kbd>` element
//! (per the WHATWG HTML spec: "user input from a keyboard"), and
//! `KeyboardHint` wraps the keys and the descriptive label in a flex row.

#![allow(non_snake_case)]
#![allow(dead_code)]

use dioxus::prelude::*;

/// Visual size of a [`Kbd`] keycap.
///
/// `Sm` (the default) is used in help rows and select popover footers.
/// `Xs` is the compact variant used in the footer's keyboard-hints strip.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum KbdSize {
    /// Compact keycap — 9px font, tighter padding. Used in dense surfaces
    /// such as the footer keyboard hints strip.
    Xs,
    /// Default keycap size — 11px font, standard padding.
    #[default]
    Sm,
}

impl KbdSize {
    /// Tailwind utility classes that vary with size: padding, font-size,
    /// font-family, text color, and (for Xs) explicit height. Background,
    /// border, radius, and font-weight stay constant across sizes.
    fn classes(self) -> &'static str {
        match self {
            // Compact: footer keyboard-hints strip.
            KbdSize::Xs => "h-4 px-1 py-0 text-[9px] min-w-[16px] font-ui text-ui-base-muted",
            // Default: help rows and select popover footers.
            KbdSize::Sm => {
                "px-[5px] py-px text-[9.5px] min-w-[14px] font-mono text-ui-base-content"
            }
        }
    }
}

/// A single keycap rendered as a semantic `<kbd>` element.
#[component]
pub fn Kbd(label: String, #[props(default)] size: KbdSize) -> Element {
    let base = "inline-flex items-center justify-center rounded-ui-xs border \
                border-ui-border bg-ui-base-200 font-semibold";
    let size_classes = size.classes();

    rsx! {
        kbd {
            class: "{base} {size_classes}",
            "{label}"
        }
    }
}

/// A keyboard hint — one or more [`Kbd`] keycaps followed by a short
/// descriptive label.
///
/// Renders a horizontal flex row; multiple `KeyboardHint`s are typically
/// composed inside an outer flex container (e.g. the `.keyboard-hints`
/// wrapper in the footer) by the call site itself, keeping this component
/// free of layout concerns beyond a single hint.
#[component]
pub fn KeyboardHint(keys: Vec<String>, label: String) -> Element {
    rsx! {
        div {
            class: "inline-flex items-center gap-1 text-[10px] text-ui-base-muted",
            for key in keys {
                Kbd { label: key }
            }
            span { "{label}" }
        }
    }
}
