//! [`ReactionChip`] — read-only Slack reaction pill (emoji + count).
//!
//! ## Design system note: the CSS class hook hybrid
//!
//! The "mine" variant (the current user reacted) renders an extra
//! `.th-rxn--mine` class on the outer span. The stylesheet binds to it
//! with a `color-mix()`-based tint:
//!
//! ```css
//! .th-rxn--mine {
//!     background: var(--ui-primary-subtle);
//!     border-color: color-mix(in oklab, var(--ui-primary) 40%, transparent);
//!     color: var(--ui-primary-hover);
//! }
//! ```
//!
//! `color-mix()` cannot be expressed as a Tailwind utility without losing
//! token fidelity, so the highlight stays in CSS and the component owns
//! the React-style API + variant safety. The class string is the contract
//! between the two layers — keep it intact.
//!
//! ## Usage
//!
//! ```ignore
//! ReactionChip {
//!     emoji: rsx! { SlackEmojiDisplay { emoji_name, slack_references } },
//!     count: reaction.count,
//! }
//! ReactionChip {
//!     emoji: rsx! { SlackEmojiDisplay { emoji_name, slack_references } },
//!     count: reaction.count,
//!     variant: ReactionVariant::Mine,
//! }
//! ```

#![allow(non_snake_case)]

use dioxus::prelude::*;

/// Visual variant for [`ReactionChip`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ReactionVariant {
    /// Default chip — muted neutral fill on the surface.
    #[default]
    Default,
    /// "Mine" variant — the current user reacted. Emits the load-bearing
    /// `.th-rxn--mine` class for the `color-mix()` highlight.
    Mine,
}

/// Read-only Slack reaction pill — emoji slot + numeric count.
///
/// **Hybrid pattern**: when `variant == ReactionVariant::Mine`, the outer
/// `<span>` emits the `th-rxn--mine` class so the stylesheet's `color-mix()`
/// highlight binds correctly. The shell shape (border, padding, radius,
/// typography) composes from Tailwind utilities against the `--ui-*` tokens.
#[component]
pub fn ReactionChip(
    /// The emoji slot — typically a Slack emoji render (image or unicode glyph).
    emoji: Element,
    /// Reactor count.
    count: u32,
    /// Visual variant. Defaults to `Default`.
    #[props(default)]
    variant: ReactionVariant,
) -> Element {
    // Shell utilities — match the dropped `.th-rxn` rule property-for-property
    // (gap-1 = 4px, px-2 = 8px, h-[22px] from CSS, rounded-ui-pill, border,
    // bg-ui-base-200, text-ui-base-muted, font-size 10px via --ui-text-xs).
    let base_class = "inline-flex items-center gap-1 h-[22px] px-2 rounded-ui-pill \
                      border border-ui-border bg-ui-base-200 text-ui-base-muted \
                      text-[10px] tabular-nums leading-none";

    let class = match variant {
        ReactionVariant::Default => base_class.to_string(),
        // `th-rxn--mine` is load-bearing: the CSS rule
        // `.th-rxn--mine { background: …; border-color: color-mix(…); color: … }`
        // overrides the muted shell with a `color-mix()`-based highlight.
        ReactionVariant::Mine => format!("{base_class} th-rxn--mine"),
    };

    rsx! {
        span {
            class: "{class}",
            // `.th-rxn-emoji` inlined — fixed 14px height + line-height:1 on
            // the wrapper and its immediate child (so image/text render at the
            // same row baseline as the count).
            span {
                class: "inline-flex items-center h-[14px] leading-none \
                        [&>*]:h-[14px] [&>*]:leading-none \
                        [&_img]:w-[14px] [&_img]:h-[14px] [&_img]:object-contain \
                        [&_span]:inline-flex [&_span]:items-center [&_span]:text-[14px]",
                {emoji}
            }
            span { "{count}" }
        }
    }
}
