//! [`ParticipantStack`] — overlapping colored dots representing thread
//! participants, with an overflow `+N` indicator.
//!
//! ## Design system note: the CSS class hook hybrid
//!
//! The horizontal `-6px` overlap between adjacent dots is implemented via
//! a sibling-cascade CSS rule:
//!
//! ```css
//! .th-participant-stack > * + * { margin-left: -6px; }
//! ```
//!
//! Sibling cascade selectors cannot be expressed as Tailwind utilities
//! without per-child `style=` overrides that would defeat the design system,
//! so the cascade stays in CSS and the component owns the API. The outer
//! `<span class="th-participant-stack">` is **load-bearing** — keep it.
//!
//! ## Usage
//!
//! ```ignore
//! ParticipantStack {
//!     participants: vec![(id, name), …],
//!     max_visible: 4,
//! }
//! ```
//!
//! Each dot uses a deterministic HSL hue derived from the user id (caller
//! supplies the hue via `style="background: hsl(...)"`).

#![allow(non_snake_case)]

use dioxus::prelude::*;

/// One avatar dot — identifier + display name + the inline color computed
/// upstream from a deterministic hash of the user id.
#[derive(Clone, PartialEq, Eq)]
pub struct ParticipantDescriptor {
    /// Stable identifier (used as `key` and as the `title` fallback).
    pub id: String,
    /// Display name shown via the `title` attribute on hover.
    pub name: String,
    /// Background color (typically `hsl(<hue> 70% 62%)` from a user-id hash).
    pub color: String,
}

/// Overlapping colored dots for thread participants, with an overflow
/// indicator (`+N`) when there are more than `max_visible` participants.
///
/// **Hybrid pattern**: emits `class="th-participant-stack"` on the outer
/// `<span>` so the stylesheet's `> * + * { margin-left: -6px }` sibling
/// cascade applies the overlap. Do not collapse the class hook — the
/// overlap would disappear.
#[component]
pub fn ParticipantStack(
    /// All participants in canonical order. The first `max_visible` are
    /// rendered as dots; the remainder collapses into a `+N` indicator.
    participants: Vec<ParticipantDescriptor>,
    /// Cap on how many dots render before collapsing into `+N`. Defaults
    /// to 4 to match the existing thread head layout.
    #[props(default = 4)]
    max_visible: usize,
) -> Element {
    let visible_count = participants.len().min(max_visible);
    let visible: Vec<ParticipantDescriptor> =
        participants.iter().take(visible_count).cloned().collect();
    let extra = participants.len().saturating_sub(max_visible);

    rsx! {
        // `th-participant-stack` is load-bearing for the sibling-cascade
        // overlap (`.th-participant-stack > * + * { margin-left: -6px }`).
        // Layout shell composes from utilities here.
        span {
            class: "th-participant-stack inline-flex items-center",
            for p in visible.iter() {
                {
                    let id = p.id.clone();
                    let name = p.name.clone();
                    // Inline `style="background: …"` is the legitimate dynamic
                    // exception — the hue is derived per-id at render time.
                    let style = format!("background: {};", p.color);
                    rsx! {
                        span {
                            key: "{id}",
                            class: "w-4 h-4 rounded-ui-sm border-[1.5px] border-ui-surface \
                                    box-content shrink-0",
                            style: "{style}",
                            title: "{name}",
                        }
                    }
                }
            }
            if extra > 0 {
                span {
                    class: "ml-1 px-1.5 py-px rounded-ui-pill bg-ui-base-200 \
                            text-ui-base-muted text-[9px] font-semibold",
                    "+{extra}"
                }
            }
        }
    }
}
