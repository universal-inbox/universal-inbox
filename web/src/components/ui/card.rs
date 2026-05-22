#![allow(non_snake_case)]
#![allow(dead_code)]

//! Card compound component family.
//!
//! Three card surfaces map to three outer class hooks:
//!
//! - [`CardVariant::Default`]     → inline Tailwind utility composition
//!   (surface + border + radius + padding + margin-bottom)
//! - [`CardVariant::ApiKeys`]     → inline Tailwind utility composition
//! - [`CardVariant::Integration`] → `.integration-card` (collapsible row)
//!
//! The `Integration` variant emits `.integration-card` and
//! `.integration-card.expanded` so the `card-body-in` keyframe animation
//! (defined in `web/css/universal-inbox.css` and bound to
//! `.integration-card.expanded .card-body-expandable`) fires on expand.
//! Likewise, [`CardBody`] with `expandable: true` emits
//! `.card-body-expandable` so that selector resolves.
//!
//! ## Composition example
//!
//! ```ignore
//! use crate::components::ui::card::{
//!     Card, CardBody, CardEmptyState, CardHeader, CardMeta, CardRight, CardVariant,
//! };
//!
//! rsx! {
//!     Card {
//!         variant: CardVariant::Integration,
//!         expanded: true,
//!         CardHeader {
//!             // brand tile / icon goes here
//!             CardMeta {
//!                 name: "GitHub".to_string(),
//!                 description: rsx! { "Synced 2 minutes ago" },
//!             }
//!             CardRight {
//!                 // status pill, chevron, etc.
//!             }
//!         }
//!         CardBody {
//!             expandable: true,
//!             // expanded content (configuration, connection rows, …)
//!         }
//!     }
//! }
//! ```

use dioxus::prelude::*;

/// Visual variant of [`Card`].
///
/// Each variant corresponds to a distinct CSS surface class. The variant
/// decides the outer class so the CSS rules (hover, focus, expand, error
/// states, animations) bind correctly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CardVariant {
    /// Generic preview surface — emits inline Tailwind utility composition
    /// (surface background, border, large radius, 12px padding, 10px
    /// margin-bottom). No legacy class hook is needed.
    #[default]
    Default,
    /// Settings "API keys" / authentication card — emits inline Tailwind
    /// utility composition (surface + border + rounded + `notif-enter`
    /// animation). No legacy class hook is needed.
    ApiKeys,
    /// Collapsible integration row — emits `.integration-card` (plus
    /// `.expanded` when the `expanded` prop is true).
    Integration,
}

impl CardVariant {
    fn base_class(self) -> &'static str {
        match self {
            CardVariant::Default => {
                "bg-ui-surface border border-ui-border rounded-ui-lg p-3 mb-2.5"
            }
            CardVariant::ApiKeys => {
                "bg-ui-surface border border-ui-border \
                                    rounded-ui-lg overflow-hidden \
                                    [animation:notif-enter_0.2s_var(--ui-ease-out)_backwards] \
                                    [animation-delay:60ms]"
            }
            CardVariant::Integration => "integration-card",
        }
    }
}

/// Outer card container.
///
/// Renders a `<div>` with the class hook for the chosen [`CardVariant`].
/// Use the `class` prop to add modifiers such as `has-error` or
/// `disconnected-card` on the integration variant.
///
/// The `expanded` flag only has an effect for [`CardVariant::Integration`];
/// when true it appends the `expanded` class so the `card-body-in` animation
/// (`.integration-card.expanded .card-body-expandable`) fires.
///
/// ```ignore
/// rsx! {
///     Card {
///         variant: CardVariant::Integration,
///         expanded: is_expanded(),
///         class: "has-error".to_string(),
///         CardHeader { /* … */ }
///         CardBody { expandable: true, /* … */ }
///     }
/// }
/// ```
#[component]
pub fn Card(
    children: Element,
    #[props(default)] variant: CardVariant,
    #[props(default)] expanded: bool,
    class: Option<String>,
) -> Element {
    let mut classes = String::from(variant.base_class());
    if expanded && matches!(variant, CardVariant::Integration) {
        classes.push_str(" expanded");
    }
    if let Some(extra) = class.as_ref()
        && !extra.is_empty()
    {
        classes.push(' ');
        classes.push_str(extra);
    }

    rsx! {
        div { class: "{classes}", {children} }
    }
}

/// Header row inside a [`Card`].
///
/// Renders a `<div>` with Tailwind utility composition emitting a flex row
/// (gap 10px, padding 12px 14px) with hover/focus chrome on the clickable
/// variant. Pass `interactive: false` for non-clickable headers (e.g. the
/// default task manager row) — cursor + hover + focus chrome are skipped.
///
/// The header is a `group` so descendants can react to its hover state via
/// `group-hover:` utilities (used by the chevron on integration cards).
///
/// Compose the children freely — typical composition is
/// `BrandTile` + [`CardMeta`] + [`CardRight`].
///
/// ```ignore
/// rsx! {
///     CardHeader {
///         interactive: false,
///         class: "max-md:flex-wrap".to_string(),
///         CardMeta { name: "Default task manager".to_string(), description: rsx! {} }
///         CardRight { /* picker */ }
///     }
/// }
/// ```
#[component]
pub fn CardHeader(
    children: Element,
    /// Whether the header reacts to hover / click. Defaults to `true`; pass
    /// `false` for static rows that contain their own controls (e.g. the
    /// default task manager row whose picker is the interactive element).
    #[props(default = true)]
    interactive: bool,
    class: Option<String>,
) -> Element {
    let base = "group flex items-center gap-2.5 px-3.5 py-3 \
                transition-colors duration-[var(--ui-dur-fast)]";
    let interactive_classes = if interactive {
        " cursor-pointer select-none hover:bg-ui-surface-hover \
          focus-visible:outline-2 focus-visible:outline-ui-primary \
          focus-visible:-outline-offset-2 focus-visible:rounded-ui-lg"
    } else {
        ""
    };
    let extra = class.as_deref().unwrap_or("").trim();
    let classes = if extra.is_empty() {
        format!("{base}{interactive_classes}")
    } else {
        format!("{base}{interactive_classes} {extra}")
    };

    rsx! {
        div { class: "{classes}", {children} }
    }
}

/// Name + description block inside a [`CardHeader`].
///
/// Renders the name (13.5px bold) and, if provided, the description (11px
/// muted) using Tailwind utility composition. The description is an
/// [`Element`] so callers can compose rich content (status pills, links,
/// etc.) and not only plain strings.
///
/// Pass `muted_name: true` for disconnected-state cards where the name
/// should render in the muted color. Pass `hide_description: true` (e.g.
/// when the parent card is expanded) to suppress the description without
/// removing it from the tree — the body itself supersedes the summary.
///
/// ```ignore
/// rsx! {
///     CardMeta {
///         name: "GitHub".to_string(),
///         description: rsx! { "Synced 2 minutes ago" },
///         hide_description: is_expanded,
///         muted_name: !has_connection,
///     }
/// }
/// ```
#[component]
pub fn CardMeta(
    name: String,
    description: Option<Element>,
    /// Render the name in muted color (for disconnected cards).
    #[props(default = false)]
    muted_name: bool,
    /// Hide the description (used when the parent card is expanded so the
    /// expanded body content supersedes the collapsed summary).
    #[props(default = false)]
    hide_description: bool,
) -> Element {
    let name_color = if muted_name {
        "text-ui-base-muted"
    } else {
        "text-ui-base-content"
    };
    let name_classes = format!(
        "text-[13.5px] font-bold tracking-[-0.01em] {name_color} flex items-center gap-1.5"
    );

    rsx! {
        div { class: "flex-1 min-w-0",
            div { class: "{name_classes}", "{name}" }
            if let Some(desc) = description
                && !hide_description
            {
                div { class: "text-[11px] text-ui-base-muted mt-px", {desc} }
            }
        }
    }
}

/// Right-aligned cluster inside a [`CardHeader`].
///
/// Renders `<div class="flex items-center gap-2 shrink-0 …">`, the home
/// for status leaves, action buttons, dropdowns, and the chevron on
/// integration cards. Pass extra modifier classes (e.g. responsive
/// utilities like `max-md:basis-full`) via the optional `class` prop.
///
/// ```ignore
/// rsx! {
///     CardRight {
///         class: "max-md:basis-full".to_string(),
///         StatusLeaf { variant: StatusLeafVariant::Connected, label: "Connected".to_string() }
///         span { class: "size-6 inline-flex items-center justify-center rounded-full text-ui-base-muted",
///             span { class: "icon-[lucide--chevron-down] size-4" }
///         }
///     }
/// }
/// ```
#[component]
pub fn CardRight(children: Element, class: Option<String>) -> Element {
    let base = "flex items-center gap-2 shrink-0";
    let extra = class.as_deref().unwrap_or("").trim();
    let classes = if extra.is_empty() {
        base.to_string()
    } else {
        format!("{base} {extra}")
    };

    rsx! {
        div { class: "{classes}", {children} }
    }
}

/// Card body / content area.
///
/// When `expandable` is true the body is rendered with the
/// `.card-body-expandable` class so the existing CSS rule
/// `.integration-card.expanded .card-body-expandable` can show/hide it and
/// fire the `card-body-in` keyframe animation. When false the body is a
/// plain content slot — useful for the always-visible body of `ApiKeys` or
/// `Default` cards.
///
/// ```ignore
/// rsx! {
///     CardBody {
///         expandable: true,
///         div { class: "connections-list", /* … */ }
///     }
/// }
/// ```
#[component]
pub fn CardBody(
    children: Element,
    #[props(default)] expandable: bool,
    class: Option<String>,
) -> Element {
    let extra = class.as_deref().unwrap_or("").trim();
    let mut classes = String::new();
    if expandable {
        classes.push_str("card-body-expandable");
    }
    if !extra.is_empty() {
        if !classes.is_empty() {
            classes.push(' ');
        }
        classes.push_str(extra);
    }

    if classes.is_empty() {
        // No classes needed — render a bare wrapper so children still get a
        // single block-level container suitable for padding via parent CSS.
        rsx! { div { {children} } }
    } else {
        rsx! { div { class: "{classes}", {children} } }
    }
}

/// Empty-state placeholder for a card body.
///
/// Renders an inline Tailwind utility composition that mirrors the muted
/// padding/colour treatment previously provided by `.api-keys-empty-state`.
/// Pass an Iconify class via `icon_class` to render an inline glyph above
/// the title.
///
/// ```ignore
/// rsx! {
///     CardEmptyState {
///         icon_class: Some("icon-[lucide--key-round]".to_string()),
///         title: "No API keys yet".to_string(),
///         description: Some("Create one to start automating your inbox.".to_string()),
///     }
/// }
/// ```
#[component]
pub fn CardEmptyState(
    title: String,
    icon_class: Option<String>,
    description: Option<String>,
) -> Element {
    rsx! {
        div { class: "pt-3 px-4 pb-4 text-xs text-ui-base-muted opacity-60",
            if let Some(icon) = icon_class.as_ref() {
                span { class: "{icon} size-4", "aria-hidden": "true" }
            }
            div { "{title}" }
            if let Some(desc) = description.as_ref() {
                div { "{desc}" }
            }
        }
    }
}
