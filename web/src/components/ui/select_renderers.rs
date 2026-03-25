//! Reusable visual presets for [`UISelect`] and [`UISearchSelect`] rows /
//! triggers.
//!
//! Each preset comes as a `*Value` (closed trigger) and `*Option` (popover row)
//! pair so closed and open states stay visually consistent. Consumers wrap
//! these in `Callback`s tied to their data shape:
//!
//! ```ignore
//! let render_option = use_callback(|(opt, q): (UISelectOption<MyT>, String)| {
//!     rsx! { PriorityOption { color: opt.value.color(), label: opt.label.clone(), query: q } }
//! });
//! ```

#![allow(non_snake_case)]

use dioxus::prelude::*;

use super::select::{
    POP_ITEM_LABEL_CLASSES, POP_ITEM_META_CLASSES, POP_ITEM_MONO_CLASSES, ss_highlight,
};

/// Small 22×22 emoji-in-tile leading visual used by [`EmojiValue`] / [`EmojiOption`].
const EMOJI_TILE_CLASSES: &str = "inline-flex items-center justify-center w-[22px] h-[22px] \
    bg-ui-surface border border-ui-border rounded-[5px] text-[13px] leading-none shrink-0";

/// 10px color dot used by [`PriorityValue`] / [`PriorityOption`]; the dot
/// color is set via inline `style="background:{color}"` per call site.
const PRIORITY_DOT_CLASSES: &str = "w-2.5 h-2.5 rounded-full inline-block shrink-0";

// ─── Emoji ──────────────────────────────────────────────────────────────────

#[component]
pub fn EmojiValue(emoji: String, label: String) -> Element {
    rsx! {
        span { class: EMOJI_TILE_CLASSES, "{emoji}" }
        span { "{label}" }
    }
}

#[component]
pub fn EmojiOption(emoji: String, label: String, #[props(default)] query: String) -> Element {
    rsx! {
        span { class: EMOJI_TILE_CLASSES, "{emoji}" }
        span { class: POP_ITEM_MONO_CLASSES,
            { ss_highlight(&label, &query) }
        }
    }
}

// ─── Priority (color dot) ───────────────────────────────────────────────────

#[component]
pub fn PriorityValue(color: String, label: String) -> Element {
    rsx! {
        span {
            class: PRIORITY_DOT_CLASSES,
            style: "background:{color};",
        }
        span { "{label}" }
    }
}

#[component]
pub fn PriorityOption(
    color: String,
    label: String,
    #[props(default)] meta: Option<String>,
    #[props(default)] query: String,
) -> Element {
    rsx! {
        span {
            class: PRIORITY_DOT_CLASSES,
            style: "background:{color};",
        }
        span { class: POP_ITEM_LABEL_CLASSES,
            { ss_highlight(&label, &query) }
        }
        if let Some(m) = &meta {
            span { class: POP_ITEM_META_CLASSES, "{m}" }
        }
    }
}

// ─── Task manager (logo + label) ────────────────────────────────────────────
//
// Task manager logos are full-color brand glyphs rendered by Rust components
// (Todoist, TickTick) — not Iconify classes. The renderers accept a pre-built
// `Element` for the logo so callers can wire whatever brand component they
// need without this preset depending on integration internals.

#[component]
pub fn TaskMgrValue(logo: Element, label: String) -> Element {
    rsx! {
        span {
            // Flex wrapper centers the brand tile alongside the label baseline.
            // No `size-*` constraint — the logo (typically `BrandTile`) carries
            // its own dimensions.
            class: "inline-flex items-center shrink-0",
            "aria-hidden": "true",
            { logo }
        }
        span { "{label}" }
    }
}

#[component]
pub fn TaskMgrOption(logo: Element, label: String, #[props(default)] query: String) -> Element {
    rsx! {
        span {
            class: "inline-flex items-center shrink-0",
            "aria-hidden": "true",
            { logo }
        }
        span { class: POP_ITEM_LABEL_CLASSES,
            { ss_highlight(&label, &query) }
        }
    }
}
