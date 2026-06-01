//! Small pill components shared across the app — `Badge`, `Tag`, `StatusLeaf`.
//!
//! ## Design system note: the CSS class hook hybrid
//!
//! [`Tag`] emits the CSS class names (`tag review`, …) that the stylesheet
//! binds to for `::before` pseudo-element decorations, e.g.
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
//! [`Badge`] and [`StatusLeaf`], by contrast, have no pseudo-element
//! decoration, so they compose purely from utility classes + design tokens.
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

/// Visual variant for [`StatusLeaf`]. Each variant maps to a pair of
/// Tailwind utility-class strings (pill + dot) — see `container_classes`
/// and `dot_classes` below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusLeafVariant {
    /// Healthy connection — green dot + subtle green fill.
    Connected,
    /// Provider connected but unauthenticated / configuration drift.
    Disconnected,
    /// Sync error — red dot + subtle red fill.
    Error,
    /// OAuth still valid but last sync(s) failed — amber dot + subtle amber fill.
    SyncIssue,
    /// Sync in progress — neutral muted styling.
    Syncing,
}

impl StatusLeafVariant {
    /// Tailwind utility classes for the outer pill (text + background).
    fn container_classes(self) -> &'static str {
        match self {
            StatusLeafVariant::Connected => "text-ui-success bg-ui-success-subtle",
            StatusLeafVariant::Disconnected => "text-ui-base-muted bg-ui-secondary-subtle",
            StatusLeafVariant::Error => "text-ui-error bg-ui-error-subtle",
            StatusLeafVariant::SyncIssue => "text-ui-warning-text bg-ui-warning-subtle",
            StatusLeafVariant::Syncing => "text-ui-base-muted bg-ui-secondary-subtle",
        }
    }

    /// Tailwind utility classes for the inner dot (background + optional opacity).
    fn dot_classes(self) -> &'static str {
        match self {
            StatusLeafVariant::Connected => "bg-ui-success",
            StatusLeafVariant::Disconnected => "bg-ui-base-muted opacity-50",
            StatusLeafVariant::Error => "bg-ui-error",
            StatusLeafVariant::SyncIssue => "bg-ui-warning",
            StatusLeafVariant::Syncing => "bg-ui-base-muted",
        }
    }
}

/// Connection-status pill with a leading colored dot — variants
/// `Connected` / `Disconnected` / `Error` / `SyncIssue` / `Syncing`.
///
/// Composes Tailwind utilities against the `--ui-*` design tokens (exposed
/// via `@theme`). The dot is a real inner `<span>` (not a `::before`), so
/// utility classes are sufficient — no `.status-leaf` CSS hook needed.
///
/// Mobile (max-md): pill shrinks to 10px font + 7px horizontal padding via
/// responsive variants.
///
/// ```ignore
/// rsx! { StatusLeaf { variant: StatusLeafVariant::Connected, label: "Connected".into() } }
/// rsx! { StatusLeaf { variant: StatusLeafVariant::Error, label: "Auth error".into() } }
/// ```
#[component]
pub fn StatusLeaf(
    /// Which variant to render. Drives both the pill's color and the inner dot.
    variant: StatusLeafVariant,
    /// Label to show next to the dot (e.g. "Connected", "Auth error").
    label: String,
) -> Element {
    let container = variant.container_classes();
    let dot = variant.dot_classes();
    rsx! {
        span {
            class: "inline-flex items-center gap-1 whitespace-nowrap rounded-[12px] \
                    px-2 py-[3px] max-md:px-[7px] text-[10.5px] max-md:text-[10px] \
                    font-medium {container}",
            span { class: "w-1.5 h-1.5 rounded-full shrink-0 {dot}" }
            "{label}"
        }
    }
}
