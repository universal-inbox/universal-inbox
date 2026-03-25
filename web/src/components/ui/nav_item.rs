//! Sidebar navigation primitives — `NavSection`, `NavItem`, and the shared
//! Tailwind class constants that compose them.
//!
//! ## Theming
//!
//! Both components compose Tailwind v4 utilities against the sidebar tokens
//! exposed via `@theme inline` in `web/css/universal-inbox.css`:
//! `bg-sidebar-bg`, `text-sidebar-text`, `text-sidebar-text-muted`,
//! `text-sidebar-text-bright`, `bg-sidebar-hover-bg`, `bg-sidebar-active-bg`,
//! `text-sidebar-active-text`. No raw color literals.
//!
//! ## Active-state strategy
//!
//! [`NavItem`] uses Dioxus's [`Link`] with `active_class`: when the current
//! route matches the `to` prop, the framework appends [`NAV_LINK_ACTIVE`]. The
//! leading icon picks up the active state via Tailwind's `group` variant — the
//! link is the `group` and the icon uses
//! `group-[.active]:opacity-100 group-[.active]:text-sidebar-active-text` to
//! mirror the active-state cascade.
//!
//! For nav links whose active state spans multiple routes (e.g. Inbox =
//! `NotificationsPage` *or* `NotificationPage { .. }`), use a manual `<Link>`
//! in `sidebar.rs` and compose [`NAV_LINK_BASE`] / [`NAV_LINK_ACTIVE`] /
//! [`NAV_ICON_BASE`] directly so the styling stays in lockstep.
//!
//! ## Collapsed state
//!
//! When the sidebar is collapsed, the parent `<aside>` carries
//! `class="sidebar collapsed"`. Descendants react via Tailwind v4 arbitrary
//! parent variants — `md:[.sidebar.collapsed_&]:hidden` for labels and badges,
//! `md:[.sidebar.collapsed_&]:justify-center` / `:p-2` / `:text-lg` for the
//! icon-only layout. The variants are scoped to `md:` (≥768px) so the mobile
//! drawer renders fully expanded even when the persisted collapse flag is on.
//! No CSS class hooks.
//!
//! ## Usage
//!
//! ```ignore
//! use crate::components::ui::{NavSection, NavItem, Badge, BadgeVariant};
//! use crate::route::Route;
//!
//! rsx! {
//!     NavSection { label: "Manage".to_string(),
//!         NavItem {
//!             icon_class: "icon-[lucide--settings] size-4".to_string(),
//!             label: "Settings".to_string(),
//!             to: Route::SettingsPage {},
//!         }
//!         NavItem {
//!             icon_class: "icon-[lucide--shield] size-4".to_string(),
//!             label: "Security".to_string(),
//!             to: Route::SecurityPage {},
//!             badge: rsx! { Badge { variant: BadgeVariant::Primary, "3" } },
//!         }
//!     }
//! }
//! ```

#![allow(non_snake_case)]
#![allow(dead_code)]

use dioxus::prelude::*;

use crate::route::Route;

// ─── Shared Tailwind class constants ───────────────────────────────────────

/// Base utility chain for a sidebar nav link (anchor or button). Pair with
/// [`NAV_LINK_ACTIVE`] when the link represents the current route. Reacts to
/// the parent `.sidebar.collapsed` state by collapsing to an icon-only layout.
pub const NAV_LINK_BASE: &str = "group flex items-center gap-2 px-2.5 py-1.5 rounded-ui-md \
     text-sidebar-text text-sm cursor-pointer transition-all \
     hover:bg-sidebar-hover-bg hover:text-sidebar-text-bright \
     md:[.sidebar.collapsed_&]:justify-center md:[.sidebar.collapsed_&]:p-2";

/// Utilities appended to [`NAV_LINK_BASE`] when the link is the active route.
/// The literal `active` class also serves as the selector hook for the icon's
/// `group-[.active]:` variants in [`NAV_ICON_BASE`].
pub const NAV_LINK_ACTIVE: &str =
    "active bg-sidebar-active-bg text-sidebar-active-text font-medium";

/// Base utility chain for the leading icon inside a sidebar nav link. Combine
/// with the iconify class string (e.g. `"icon-[lucide--inbox] size-4"`).
pub const NAV_ICON_BASE: &str = "text-base opacity-65 text-sidebar-text-muted shrink-0 \
     group-[.active]:opacity-100 group-[.active]:text-sidebar-active-text \
     md:[.sidebar.collapsed_&]:text-lg";

// ─── NavSection ─────────────────────────────────────────────────────────────

/// Grouped block of [`NavItem`]s introduced by an uppercase wide-tracked
/// overline label. The label hides itself when the sidebar is collapsed.
#[component]
pub fn NavSection(
    /// Section heading shown above the items (rendered uppercase).
    label: String,
    /// Nav rows belonging to this section — typically one or more [`NavItem`]s.
    children: Element,
) -> Element {
    rsx! {
        div { class: "mb-3.5",
            div {
                class: "px-2.5 py-1.5 text-xs font-semibold uppercase \
                        tracking-wider text-sidebar-text-muted \
                        md:[.sidebar.collapsed_&]:hidden",
                "{label}"
            }
            {children}
        }
    }
}

// ─── NavItem ────────────────────────────────────────────────────────────────

/// Interactive sidebar nav row: leading icon + label + optional trailing badge.
///
/// Wraps Dioxus's [`Link`] so the active state lights up automatically when
/// the current route matches `to`. The icon participates in the active state
/// via Tailwind's `group-[.active]:` variant (see module docs). The label and
/// badge collapse via the `[.sidebar.collapsed_&]:hidden` parent variant when
/// the sidebar is collapsed.
#[component]
pub fn NavItem(
    /// Iconify class string for the leading icon (e.g.
    /// `"icon-[lucide--inbox] size-4"`). Caller controls icon family + size.
    icon_class: String,
    /// Visible row label.
    label: String,
    /// Destination route. Used both as the `Link` target and as the
    /// active-state predicate (active when the current route equals `to`).
    to: Route,
    /// Optional trailing element rendered flush-right (typically a
    /// [`crate::components::ui::Badge`]).
    #[props(default)]
    badge: Option<Element>,
) -> Element {
    let icon_classes = format!("{icon_class} {NAV_ICON_BASE}");

    rsx! {
        Link {
            class: "{NAV_LINK_BASE}",
            active_class: "{NAV_LINK_ACTIVE}",
            to,
            span { class: "{icon_classes}" }
            span { class: "md:[.sidebar.collapsed_&]:hidden", "{label}" }
            if let Some(badge) = badge {
                span { class: "ml-auto md:[.sidebar.collapsed_&]:hidden", {badge} }
            }
        }
    }
}
