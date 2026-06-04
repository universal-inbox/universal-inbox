//! [`StatusSection`] / [`StatusRow`] / [`StatusDot`] — collapsible status
//! section used by detail preview panes (Google Calendar attendees / RSVP and
//! GitHub PR reviewers + CI checks).
//!
//! ## Shape
//!
//! - [`StatusSection`] — clickable header (dot slot + label + summary +
//!   chevron) + collapsible body slot. Chevron rotates 180° when open;
//!   state is local via `use_signal` with a `use_effect` re-sync on the
//!   `expand` signal so a parent-controlled "expand all" signal stays
//!   authoritative.
//! - [`StatusRow`] — flex row with `space-between`. `variant` drives the
//!   background tint (error → `bg-ui-error-subtle`, warning →
//!   `bg-ui-warning-subtle`, default/success → transparent).
//! - [`StatusDot`] — 8x8 pill colored by `StatusVariant` (`Default` →
//!   `bg-ui-base-300`, `Success` → `bg-ui-success`, `Warning` →
//!   `bg-ui-warning`, `Error` → `bg-ui-error`).
//!
//! Replaces the entire `.preview-status-*` CSS family (head shell, chevron
//! cascade, dot variants, list flex column, row variant tints, row name +
//! row action descendant cascades). Chevron rotation is now Dioxus
//! signal-driven; the legacy `.chevron.open` attribute-selector cascade
//! is gone.
//!
//! ## Usage
//!
//! ```ignore
//! StatusSection {
//!     dot: rsx! { StatusDot { variant: StatusVariant::Success } },
//!     label: "Guests",
//!     summary: "4 guests · 3 yes, 1 awaiting",
//!     expand: expand_details,
//!     for attendee in attendees {
//!         StatusRow { variant: StatusVariant::Default,
//!             AttendeeBody { attendee }
//!         }
//!     }
//! }
//! ```

#![allow(non_snake_case)]

use dioxus::prelude::*;

/// Variant for status rows and dots — drives row background tint and dot
/// color. `Default` (and `Success` for rows) leaves the row transparent so
/// the underlying card surface shows through; the dot still picks up the
/// variant color.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum StatusVariant {
    /// No tint, neutral dot.
    #[default]
    Default,
    /// Success dot; row stays transparent (success is the resting state).
    Success,
    /// Warning dot + subtle warning bg tint.
    Warning,
    /// Error dot + subtle error bg tint.
    Error,
}

impl StatusVariant {
    fn dot_class(self) -> &'static str {
        match self {
            Self::Default => "bg-ui-base-300",
            Self::Success => "bg-ui-success",
            Self::Warning => "bg-ui-warning",
            Self::Error => "bg-ui-error",
        }
    }

    fn row_tint_class(self) -> &'static str {
        match self {
            // Default + Success rows stay transparent so the row blends into
            // the surrounding card surface.
            Self::Default | Self::Success => "",
            Self::Warning => "bg-ui-warning-subtle",
            Self::Error => "bg-ui-error-subtle",
        }
    }
}

/// Small 8x8 status dot — used in the [`StatusSection`] header and inline in
/// status rows. Matches the dropped `.preview-status-dot` rule property for
/// property: 8x8, full radius, color from the `--ui-*` token map.
#[component]
pub fn StatusDot(variant: StatusVariant) -> Element {
    let class = format!("size-2 rounded-full flex-none {}", variant.dot_class());
    rsx! { div { class: "{class}" } }
}

/// Collapsible status section — clickable header with a dot, a bold label,
/// a muted summary, and a chevron that rotates 180° when expanded. The body
/// renders only when the section is open.
///
/// `dot` is a slot so the caller composes the appropriate [`StatusDot`]
/// variant. `initially_open` seeds the open state; if the parent flips it
/// later (e.g. when the user toggles "expand details"), a `use_effect`
/// re-syncs the local signal.
#[component]
pub fn StatusSection(
    /// Slot for the leading dot — typically a [`StatusDot`] with the rolled-up
    /// variant of the section.
    dot: Element,
    /// Bold label on the left ("Guests", "Reviewers", "Checks").
    label: String,
    /// Muted summary text on the right of the label
    /// ("4 guests · 3 yes, 1 awaiting", "2 successful, 1 failing").
    summary: String,
    /// Drives the open state. Read reactively inside a `use_effect` so the
    /// parent's `expand_details` signal stays authoritative — when it flips
    /// (e.g. the user presses `e` to expand details), the section re-syncs.
    /// Must be a signal: a plain `bool` prop would be captured once and never
    /// re-fire the effect.
    expand: ReadSignal<bool>,
    /// Body — typically one or more [`StatusRow`]s.
    children: Element,
) -> Element {
    let mut is_open = use_signal(move || *expand.peek());
    use_effect(move || {
        is_open.set(expand());
    });

    // Head shell — matches the dropped `.preview-status-section-head` rule:
    // flex + 8px gap, 8px vertical padding, 12.5px font, cursor-pointer,
    // non-selectable.
    let head_class = "flex items-center gap-2 py-2 cursor-pointer select-none text-[12.5px]";
    let chevron_class = if is_open() {
        // `transition-transform` + `duration-200` + `ease-[var(--ui-ease)]`
        // replicate the original 0.2s transform transition on `.chevron`.
        "ml-auto text-ui-base-muted transition-transform duration-200 ease-[var(--ui-ease)] \
         rotate-180 icon-[lucide--chevron-down] size-4"
    } else {
        "ml-auto text-ui-base-muted transition-transform duration-200 ease-[var(--ui-ease)] \
         icon-[lucide--chevron-down] size-4"
    };

    rsx! {
        div {
            class: "{head_class}",
            onclick: move |_| is_open.toggle(),
            {dot}
            span { class: "font-semibold text-ui-base-content", "{label}" }
            span { class: "text-ui-base-muted", "{summary}" }
            span { class: "{chevron_class}" }
        }

        if is_open() {
            // Body wrapper — replaces `.preview-status-list`: flex column,
            // 2px gap between rows, 8px bottom padding.
            div {
                class: "flex flex-col gap-0.5 pb-2",
                {children}
            }
        }
    }
}

/// Row inside a [`StatusSection`] — flex row with `space-between` so the
/// row name + an optional trailing action distribute. The `variant` drives
/// the row background tint; the row body is a slot so callers compose
/// whatever icon + name + action layout fits.
///
/// The caller is responsible for the row name composition; use the
/// [`status_row_name_class`] / [`status_row_action_class`] helpers (or just
/// inline the utilities) on the children to match the dropped
/// `.preview-status-row .row-name` / `.row-action` cascades.
#[component]
pub fn StatusRow(
    /// Variant — drives the background tint. Default / Success render
    /// transparent; Warning / Error tint with the matching `*-subtle` token.
    #[props(default)]
    variant: StatusVariant,
    /// Row body — typically `.row-name` (icon + label) + optional
    /// trailing action.
    children: Element,
) -> Element {
    // Shell utilities — match the dropped `.preview-status-row` rule:
    // flex justify-between, 10px gap, 6px vertical / 10px horizontal padding,
    // small radius, 12.5px font.
    let base = "flex items-center justify-between gap-2.5 px-2.5 py-1.5 \
                rounded-ui-sm text-[12.5px]";
    let tint = variant.row_tint_class();
    let class = if tint.is_empty() {
        base.to_string()
    } else {
        format!("{base} {tint}")
    };

    rsx! {
        div {
            class: "{class}",
            {children}
        }
    }
}

/// Utility class string for the `.row-name` slot inside a [`StatusRow`].
/// Matches the dropped `.preview-status-row .row-name` cascade: flex,
/// 8px gap, min-w-0 (so the text can truncate), flex-1.
///
/// The original cascade also applied `color: var(--ui-base-content)` to
/// `<a>` children with a `:hover` flip to `var(--ui-primary)`. Compose
/// `[&_a]:text-ui-base-content [&_a:hover]:text-ui-primary` on the row name
/// (or directly on the `<a>`) to preserve that behavior.
pub const STATUS_ROW_NAME_CLASS: &str = "flex items-center gap-2 min-w-0 flex-1 \
                                         [&_a]:text-ui-base-content \
                                         [&_a:hover]:text-ui-primary";

/// Utility class string for the trailing action (typically a "Details" link)
/// inside a [`StatusRow`]. Matches the dropped `.preview-status-row .row-action`
/// cascade: primary color, 12px font, no shrink, underline on hover.
pub const STATUS_ROW_ACTION_CLASS: &str = "text-ui-primary text-xs flex-none hover:underline";
