//! Label/value metadata grid used across notification preview panes.
//!
//! Two components compose: [`MetadataGrid`] is the 2-column container, and
//! [`MetadataItem`] renders a single label/value row inside it. Both compose
//! Tailwind utilities backed by `@theme` tokens — no CSS class hook required.
//!
//! ## API shape
//!
//! - `MetadataGrid` accepts `children: Element` — pass any number of
//!   [`MetadataItem`] children (or arbitrary `rsx!` blocks if a row needs
//!   custom markup). Each child contributes two grid cells (label + value).
//! - `MetadataItem` takes `label: String` and `value: Element`, so callers can
//!   pass complex value markup (links, tags, monospace spans, icons) without
//!   the component prescribing the inner structure.
//!
//! ## Usage
//!
//! ```ignore
//! use crate::components::ui::{MetadataGrid, MetadataItem};
//!
//! rsx! {
//!     MetadataGrid {
//!         MetadataItem {
//!             label: "Repository".to_string(),
//!             value: rsx!(span { "{repo}" }),
//!         }
//!         MetadataItem {
//!             label: "Branch".to_string(),
//!             value: rsx!(code { class: "preview-mono", "{branch}" }),
//!         }
//!     }
//! }
//! ```

#![allow(non_snake_case)]
#![allow(dead_code)]

use dioxus::prelude::*;

/// Two-column label/value grid. Container for [`MetadataItem`] rows used in
/// notification preview panes (GitHub PR, Linear issue, web page metadata, …).
///
/// Renders a CSS grid with a 92px label column and a flexible value column,
/// 10px row gap and 14px column gap, and 12px bottom margin. Each
/// [`MetadataItem`] child emits two sibling cells (label + value) directly
/// into this grid; no per-row wrapper is needed.
///
/// Pass any `Element` children. Typically these are [`MetadataItem`] calls,
/// but raw `rsx!` blocks producing label/value sibling pairs are also valid
/// when a row needs unusual structure.
#[component]
pub fn MetadataGrid(
    /// One or more [`MetadataItem`] rows (or equivalent label/value sibling
    /// pairs). Each contributes two cells to the grid.
    children: Element,
) -> Element {
    rsx! {
        div {
            class: "grid grid-cols-[92px_1fr] gap-y-[10px] gap-x-[14px] \
                    items-center mb-3",
            {children}
        }
    }
}

/// Single label/value row inside a [`MetadataGrid`]. Emits two sibling cells:
/// an uppercase muted label, and a flex-wrap value cell that can hold any
/// inline markup the caller passes.
///
/// Label styling: 11px, bold, uppercase, 0.04em tracking, muted text color.
///
/// Value styling: 12.5px, base content color, horizontal flex with 6px gap
/// and wrap-on-overflow. The flex container is intentional: callers routinely
/// pass multiple inline nodes (a link plus a separator plus a `<code>` token,
/// etc.) and rely on the wrapper to align and wrap them.
///
/// `value` is an `Element` (not `String`) so callers can render arbitrarily
/// complex content — anchors, tags, mono-styled tokens — without the
/// component dictating the inner structure.
#[component]
pub fn MetadataItem(
    /// Label text shown in the left column. Rendered uppercase.
    label: String,
    /// Right-column content. Typically an inline span, anchor, or a small
    /// flow of mixed inline nodes.
    value: Element,
) -> Element {
    rsx! {
        span {
            class: "text-[11px] font-bold uppercase tracking-[0.04em] \
                    text-ui-base-muted self-center",
            "{label}"
        }
        span {
            class: "text-[12.5px] text-ui-base-content flex items-center \
                    gap-1.5 flex-wrap",
            {value}
        }
    }
}
