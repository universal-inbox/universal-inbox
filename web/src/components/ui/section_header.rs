//! Section / page heading primitives for the Universal Inbox design system:
//!
//! - `PageHeader` — page-level title block with an optional subtitle and
//!   trailing actions slot.
//! - `Overline` — uppercase wide-tracked group label used inside cards,
//!   settings panels, and detail panes.
//!
//! Both components style themselves exclusively with Tailwind v4 utilities
//! backed by the project's `@theme` tokens (`text-ui-base-content`,
//! `text-ui-base-muted`, `font-ui`). No custom CSS classes are emitted.
//!
//! ## Usage
//!
//! ```ignore
//! use crate::components::ui::{Overline, PageHeader};
//!
//! // Page header — title only
//! rsx! { PageHeader { title: "Settings".to_string() } }
//!
//! // Page header — title + subtitle
//! rsx! {
//!     PageHeader {
//!         title: "Settings".to_string(),
//!         subtitle: Some("Manage integrations".to_string()),
//!     }
//! }
//!
//! // Page header — title + subtitle + trailing actions slot
//! rsx! {
//!     PageHeader {
//!         title: "Notifications".to_string(),
//!         subtitle: Some("Triage your inbox".to_string()),
//!         actions: rsx! {
//!             button { class: "btn", "Mark all as read" }
//!         },
//!     }
//! }
//!
//! // Group overline — children are the label content
//! rsx! { Overline { "Integrations" } }
//! ```
//!
//! `PageHeader` renders a semantic `<header>` containing an `<h1>` and an
//! optional `<p>` subtitle. When `actions` are provided, the header switches
//! to a flex row with the title block on the left and the actions slot on
//! the right (matching the Superhuman-style page chrome). `Overline` emits
//! a single `<div>` with text-only styling so it can be reused as a section
//! label inside cards, settings panels, and detail panes.

#![allow(non_snake_case)]
#![allow(dead_code)]

use dioxus::prelude::*;

/// Page-level title block with an optional subtitle and an optional trailing
/// actions slot.
///
/// Typography:
/// - title: `text-2xl font-bold tracking-tight text-ui-base-content`
/// - subtitle: `text-sm text-ui-base-muted`
///
/// The outer `<header>` carries `mb-[18px]` for the spacing below the
/// heading block.
#[component]
pub fn PageHeader(
    title: String,
    #[props(default)] subtitle: Option<String>,
    #[props(default)] actions: Option<Element>,
) -> Element {
    let header_class = if actions.is_some() {
        "mb-[18px] flex items-start justify-between gap-4"
    } else {
        "mb-[18px]"
    };

    rsx! {
        header { class: "{header_class}",
            div { class: "min-w-0",
                h1 {
                    class: "font-ui text-2xl font-bold tracking-tight text-ui-base-content m-0",
                    "{title}"
                }
                if let Some(subtitle) = subtitle {
                    p {
                        class: "mt-0.5 mb-0 text-sm text-ui-base-muted",
                        "{subtitle}"
                    }
                }
            }
            if let Some(actions) = actions {
                div { class: "flex items-center gap-2 shrink-0",
                    {actions}
                }
            }
        }
    }
}

/// Uppercase wide-tracked group label.
///
/// Renders a `<div>` with `text-xs font-semibold uppercase tracking-wider
/// text-ui-base-muted mb-2.5`. Children are the label content (typically
/// a short string such as `"Integrations"` or `"Account"`).
///
/// Pass extra utility classes via `class` to tune spacing relative to
/// siblings — e.g. `mt-4` to keep a 16px gap above an Overline that
/// follows another section.
#[component]
pub fn Overline(children: Element, #[props(default)] class: Option<String>) -> Element {
    let extra = class.as_deref().unwrap_or("").trim();
    let classes = if extra.is_empty() {
        "text-xs font-semibold uppercase tracking-wider text-ui-base-muted mb-2.5".to_string()
    } else {
        format!("text-xs font-semibold uppercase tracking-wider text-ui-base-muted mb-2.5 {extra}")
    };
    rsx! {
        div { class: "{classes}", {children} }
    }
}
