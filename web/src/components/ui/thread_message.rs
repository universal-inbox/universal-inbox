#![allow(non_snake_case)]

//! Threaded-message row used by every detail preview that renders a
//! conversation — Drive comments, Gmail threads, Slack threads, Linear issue
//! comments, GitHub PR comments.
//!
//! The composition is utility-driven (Tailwind v4 + design tokens) so the
//! component picks up dark mode and theme tokens automatically. A small set
//! of **load-bearing class hooks** must be kept intact:
//!
//! - `.ui-thread-msg-body` — anchors the descendant rule
//!   `.ui-thread-msg-body :where(p, ul, ol, pre) { margin: 0 }` which
//!   normalises margins inside arbitrary embedded markdown / email HTML,
//!   plus a `.prose` font-size override and an `.ui-email-frame-host`
//!   sizing rule. These target descendant DOM that cannot be addressed
//!   via utilities.
//! - `.ui-thread-msg-grouped` + `.ui-thread-msg-followup` +
//!   `.ui-thread-msg-followup-time` — anchor the 38px child indent
//!   (`.ui-thread-msg-grouped > .ui-thread-msg-followup`) and the
//!   `:hover` cascade that fades in the clock-time of follow-up rows
//!   (`.ui-thread-msg-grouped:hover .ui-thread-msg-followup-time, ...`).
//!
//! Font sizes use arbitrary values (`text-[10px]`, `text-[12.5px]`) because
//! the design system's `--ui-text-*` tokens are not registered as Tailwind
//! `--text-*` utilities.

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use url::Url;

use crate::{
    components::{avatar_hue_index, get_initials_from_name},
    utils::{format_absolute_time, format_clock_time, format_elapsed_time},
};

/// Single threaded message row. Renders avatar + author header + body.
///
/// Used by every integration preview that shows comments / replies / messages
/// (Drive, Gmail, Slack, Linear, GitHub PR). The date is always pushed to the
/// right via `ml-auto` and the relative time has the absolute timestamp as a
/// `title` tooltip.
#[component]
pub fn ThreadedMessage(
    author_name: String,
    author_avatar_url: Option<Url>,
    author_subtitle: Option<String>,
    sent_at: Option<DateTime<Utc>>,
    metadata: Option<Element>,
    body: Element,
    footer: Option<Element>,
    dimmed: Option<bool>,
) -> Element {
    let dimmed = dimmed.unwrap_or(false);
    let row_class = if dimmed {
        "flex gap-2.5 py-1 opacity-60"
    } else {
        "flex gap-2.5 py-1"
    };

    let initials = get_initials_from_name(&author_name);
    let hue_index = avatar_hue_index(&author_name);
    let avatar_inline_url = author_avatar_url.as_ref().map(|u| u.to_string());

    let (date_relative, date_absolute) = match sent_at {
        Some(dt) => (
            Some(format_elapsed_time(dt)),
            Some(format_absolute_time(dt)),
        ),
        None => (None, None),
    };

    rsx! {
        div {
            class: "{row_class}",

            ThreadedMessageAvatar {
                initials: initials.clone(),
                hue_index,
                avatar_url: avatar_inline_url,
                title: author_name.clone(),
            }

            div {
                class: "flex-1 min-w-0 flex flex-col gap-1",

                div {
                    class: "flex items-baseline gap-2 flex-wrap",
                    span {
                        class: "font-semibold text-ui-base-content text-[11px]",
                        "{author_name}"
                    }
                    if let Some(subtitle) = author_subtitle {
                        span {
                            class: "text-ui-base-muted text-[10px] font-normal",
                            "{subtitle}"
                        }
                    }
                    if let Some(rel) = date_relative {
                        span {
                            class: "ml-auto text-[10px] text-ui-base-muted shrink-0",
                            title: date_absolute.unwrap_or_default(),
                            "{rel}"
                        }
                    }
                }

                if let Some(metadata) = metadata {
                    div {
                        class: "text-ui-base-muted text-[10px] flex gap-1 flex-wrap",
                        {metadata}
                    }
                }

                // `.ui-thread-msg-body` is a kept class hook — see module doc.
                div {
                    class: "ui-thread-msg-body text-[12.5px] leading-[1.45] text-ui-base-content",
                    {body}
                }

                if let Some(footer) = footer {
                    div { class: "mt-1.5", {footer} }
                }
            }
        }
    }
}

/// Compact follow-up row used inside a `.ui-thread-msg-grouped` wrapper to
/// render same-author followup messages without repeating the avatar/header.
/// The clock-time is hidden until hover (cascade lives in CSS — see module
/// doc-comment).
#[component]
pub fn ThreadedMessageFollowup(sent_at: Option<DateTime<Utc>>, body: Element) -> Element {
    let clock = sent_at.map(format_clock_time);
    let absolute = sent_at.map(format_absolute_time);

    rsx! {
        // `.ui-thread-msg-followup` is a kept class hook for the
        // 38px indent + hover cascade — see module doc.
        div {
            class: "ui-thread-msg-followup flex items-baseline gap-2 py-0.5",
            div { class: "flex-1 min-w-0", {body} }
            if let Some(clock) = clock {
                // `.ui-thread-msg-followup-time` is the target of the
                // parent `:hover` opacity cascade — kept class hook.
                span {
                    class: "ui-thread-msg-followup-time text-[10px] text-ui-base-muted shrink-0",
                    title: absolute.unwrap_or_default(),
                    "{clock}"
                }
            }
        }
    }
}

#[component]
fn ThreadedMessageAvatar(
    initials: String,
    hue_index: u8,
    avatar_url: Option<String>,
    title: String,
) -> Element {
    // Common avatar utilities reproducing the previous `.ui-thread-msg-avatar`
    // rule: 28×28 box, medium radius, centred white bold text, double
    // inset/drop shadow for crisp lift, no shrink, hidden overflow.
    const AVATAR_CLASSES: &str = "w-7 h-7 rounded-ui-md inline-flex items-center justify-center text-white font-bold text-[10px] tracking-[0.02em] shadow-[inset_0_1px_0_rgba(255,255,255,0.18),0_1px_0_rgba(0,0,0,0.08)] shrink-0 select-none overflow-hidden";

    if let Some(url) = avatar_url {
        rsx! {
            img {
                class: "{AVATAR_CLASSES} object-cover",
                src: "{url}",
                alt: "{title}",
                title: "{title}",
            }
        }
    } else {
        let style = format!("background: var(--ui-avatar-hue-{hue_index});");
        rsx! {
            span {
                class: "{AVATAR_CLASSES}",
                style: "{style}",
                title: "{title}",
                "{initials}"
            }
        }
    }
}
