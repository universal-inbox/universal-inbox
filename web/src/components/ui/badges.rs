//! Small pill components shared across the app — `Badge`, `Tag`, `StatusLeaf`.
//!
//! ## Design system note: the CSS class hook hybrid
//!
//! [`Tag`] and [`StatusLeaf`] emit the CSS class names
//! (`tag review`, `status-leaf connected`, …) that the stylesheet binds to
//! for `::before` pseudo-element decorations, e.g.
//!
//! ```css
//! .tag.review::before { content: ""; width: 5px; height: 5px; … }
//! ```
//!
//! Pseudo-elements cannot be expressed in Tailwind utilities without losing
//! token fidelity, so the decoration stays in CSS and the component owns the
//! React-style API + variant safety. The class string is the contract between
//! the two layers — keep it intact.
//!
//! [`Badge`], by contrast, has no pseudo-element decoration, so it composes
//! purely from utility classes + design tokens.
//!
//! ## When to use each
//!
//! - [`Badge`] — generic count / label pill (nav row, profile, auth method).
//! - [`Tag`] — semantic state tag in lists and previews (open/review/urgent).
//! - [`StatusLeaf`] — integration connection-status pill with leading dot.

#![allow(non_snake_case)]
// Variants and helpers are enumerated up-front for the design-system surface,
// so some are unused for now.
#![allow(dead_code)]

use dioxus::prelude::*;

// ─── Badge ──────────────────────────────────────────────────────────────────

/// Visual variant for [`Badge`]. Maps to a dedicated style preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadgeVariant {
    /// Filled primary brand color — used for active counts (e.g. inbox unread).
    Primary,
    /// Subtle muted fill — used for secondary/ambient counts.
    Muted,
    /// Inline numeric count next to a heading. No background, just muted text.
    Count,
    /// Email verified/unverified indicator. Pair with `success`/`warning` tone
    /// via the optional `tone` prop.
    Email,
    /// Auth method indicator (Local / Passkey / Google / OIDC). Pair with the
    /// optional `tone` prop to pick the brand-specific subtle color.
    Method,
}

/// Optional semantic tone for variants that have multiple flavors
/// (`Email::{verified,unverified}`, profile success/warning, auth methods).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadgeTone {
    Success,
    Warning,
    Error,
    Info,
    Primary,
    Purple,
}

impl BadgeTone {
    fn utility_classes(self) -> &'static str {
        match self {
            BadgeTone::Success => "bg-ui-success-subtle text-ui-success",
            BadgeTone::Warning => "bg-ui-warning-subtle text-ui-warning",
            BadgeTone::Error => "bg-ui-error-subtle text-ui-error",
            BadgeTone::Info => "bg-ui-info-subtle text-ui-info",
            BadgeTone::Primary => "bg-ui-primary-subtle text-ui-primary",
            BadgeTone::Purple => "bg-ui-purple-subtle text-ui-purple",
        }
    }
}

/// Generic small pill used for nav-row counts, profile labels, email-verified
/// indicators, and auth-method indicators.
///
/// Composes Tailwind utilities against the `--ui-*` design tokens (exposed via
/// `@theme`). For variants that take semantic tones (`Email`, `Method`, or
/// any badge that can be success/warning), pass `tone`.
///
/// ```ignore
/// rsx! { Badge { variant: BadgeVariant::Primary, "12" } }
/// rsx! { Badge { variant: BadgeVariant::Email, tone: BadgeTone::Success, "Verified" } }
/// ```
#[component]
pub fn Badge(
    /// Which preset to render.
    variant: BadgeVariant,
    /// Optional semantic tone (success/warning/…). Required for `Email` and
    /// `Method`; ignored by `Primary`/`Muted`/`Count`.
    #[props(default)]
    tone: Option<BadgeTone>,
    /// Pill contents — typically a count or short label.
    children: Element,
) -> Element {
    let class = match variant {
        BadgeVariant::Primary => "inline-flex items-center justify-center min-w-[18px] h-[18px] \
             px-[5px] rounded-[9px] text-[11px] font-semibold \
             bg-ui-primary text-ui-primary-content"
            .to_string(),
        BadgeVariant::Muted => "inline-flex items-center justify-center min-w-[18px] h-[18px] \
             px-[5px] rounded-[9px] text-[11px] font-semibold \
             bg-white/[.08] text-ui-sidebar-text-muted"
            .to_string(),
        BadgeVariant::Count => "text-[11px] font-medium text-ui-base-muted px-0.5".to_string(),
        BadgeVariant::Email => {
            let tone_classes = tone.unwrap_or(BadgeTone::Success).utility_classes();
            format!(
                "inline-flex items-center gap-[3px] text-[10px] font-semibold \
                 px-[7px] py-0.5 rounded-[10px] {tone_classes}"
            )
        }
        BadgeVariant::Method => {
            let tone_classes = tone.unwrap_or(BadgeTone::Primary).utility_classes();
            format!(
                "inline-flex items-center gap-1 text-[11px] font-semibold \
                 px-2 py-0.5 rounded-ui-pill {tone_classes}"
            )
        }
    };

    rsx! {
        span { class: "{class}", {children} }
    }
}

// ─── Tag ────────────────────────────────────────────────────────────────────

/// Visual variant for [`Tag`]. The discriminant maps 1:1 to a CSS class name
/// so the stylesheet (including `::before` dot decorations) binds correctly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagVariant {
    /// Informational — open/active state (e.g. PR open, issue open).
    Open,
    /// Action-required — review requested. Renders a 5px accent-colored dot
    /// before the label via `.tag.review::before` in CSS.
    Review,
    /// Error / closed-with-failure state.
    Error,
    /// Warning state.
    Warning,
    /// Informational tone.
    Info,
    /// Success / resolved tone. Maps to `.tag.success` in CSS.
    Success,
    /// Neutral muted tone.
    Muted,
    /// Action-required — urgent. Renders a 5px error-colored dot via
    /// `.tag.urgent::before` in CSS.
    Urgent,
    /// Action-required — mention. Renders a 5px info-colored dot via
    /// `.tag.mention::before` in CSS.
    Mention,
}

impl TagVariant {
    /// CSS modifier name appended to the base `tag` class. **Load-bearing**:
    /// the stylesheet's `::before` selectors bind to it.
    fn css_modifier(self) -> &'static str {
        match self {
            TagVariant::Open => "open",
            TagVariant::Review => "review",
            TagVariant::Error => "error",
            TagVariant::Warning => "warning",
            TagVariant::Info => "info",
            TagVariant::Success => "success",
            TagVariant::Muted => "muted",
            TagVariant::Urgent => "urgent",
            TagVariant::Mention => "mention",
        }
    }
}

/// Semantic action / state tag — variants
/// `open` / `review` / `error` / `warning` / `info` / `success` / `muted` /
/// `urgent` / `mention`.
///
/// **Hybrid pattern**: emits `class="tag {modifier}"` so the stylesheet's
/// `::before` colored-dot pseudo-elements (for `review`/`urgent`/`mention`)
/// bind correctly. Do **not** replace the class hook with utility-only
/// styling — the dot decoration would disappear. See module docs for the
/// rationale.
///
/// ```ignore
/// rsx! { Tag { variant: TagVariant::Review, "Review requested" } }
/// rsx! { Tag { variant: TagVariant::Urgent, "Due today" } }
/// ```
#[component]
pub fn Tag(
    /// Which variant to render. Drives the CSS class (and any `::before` dot).
    variant: TagVariant,
    /// Tag label (typically a short string, but any inline content works).
    children: Element,
) -> Element {
    let modifier = variant.css_modifier();
    rsx! {
        span { class: "tag {modifier}", {children} }
    }
}

// ─── StatusLeaf ─────────────────────────────────────────────────────────────

/// Visual variant for [`StatusLeaf`]. Maps 1:1 to a CSS modifier on
/// `.status-leaf` (e.g. `connected`, `error`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusLeafVariant {
    /// Healthy connection — green dot + subtle green fill.
    Connected,
    /// Provider connected but unauthenticated / configuration drift.
    Disconnected,
    /// Sync error — red dot + subtle red fill.
    Error,
    /// Sync in progress. Falls back to base styles until a `.status-leaf.syncing`
    /// rule is added; safe to use today.
    Syncing,
}

impl StatusLeafVariant {
    fn css_modifier(self) -> &'static str {
        match self {
            StatusLeafVariant::Connected => "connected",
            StatusLeafVariant::Disconnected => "disconnected",
            StatusLeafVariant::Error => "error",
            StatusLeafVariant::Syncing => "syncing",
        }
    }
}

/// Connection-status pill with a leading colored dot — variants
/// `connected` / `disconnected` / `error` / `syncing`.
///
/// **Same hybrid pattern as [`Tag`]**: emits `class="status-leaf {modifier}"`
/// and an inner `<span class="leaf-dot">` so the CSS rules
/// (`.status-leaf.connected .leaf-dot { background: var(--ui-success); }`)
/// bind correctly. Do not collapse the dot into utilities.
///
/// ```ignore
/// rsx! { StatusLeaf { variant: StatusLeafVariant::Connected, label: "Connected".into() } }
/// rsx! { StatusLeaf { variant: StatusLeafVariant::Error, label: "Auth error".into() } }
/// ```
#[component]
pub fn StatusLeaf(
    /// Which variant to render. Drives both the outer modifier class and the
    /// inner `.leaf-dot` color (via the cascaded CSS rules).
    variant: StatusLeafVariant,
    /// Label to show next to the dot (e.g. "Connected", "Auth error").
    label: String,
) -> Element {
    let modifier = variant.css_modifier();
    rsx! {
        span { class: "status-leaf {modifier}",
            span { class: "leaf-dot" }
            "{label}"
        }
    }
}
