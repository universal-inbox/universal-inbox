//! Shared modal chrome for the task-planning and task-link modals.
//!
//! Three Dioxus components — `ModalHeader`, `ModalSourceRow`, `ModalFooter` —
//! composed from Tailwind v4 utilities + design tokens (`bg-ui-*`,
//! `border-ui-*`, `rounded-ui-*`, `shadow-ui-*`).
//!
//! These components are scoped to the popin-style modal pattern shared by
//! `TaskPlanningModal` and `TaskLinkModal` (eyebrow + title header, gray
//! "source" banner, gradient footer with Tab/Esc hint and action buttons).
//! They are **not** intended as a generic modal abstraction — the confirmation
//! modal (`DeleteAllConfirmationModal`) uses FlyonUI's stock
//! `.modal-header` / `.modal-body` / `.modal-footer` instead.

#![allow(non_snake_case)]
#![allow(dead_code)]

use dioxus::prelude::*;

/// Modal header with an UPPERCASE eyebrow, an h2 title, and a top-right close
/// button wired to FlyonUI's `data-overlay` close target.
#[component]
pub fn ModalHeader(
    eyebrow: String,
    title: String,
    title_id: String,
    overlay_id: String,
) -> Element {
    rsx! {
        header { class: "relative px-4 pt-3.5 pb-3 border-b border-ui-border-light rounded-t-ui-lg",
            div { class: "text-[10px] font-semibold uppercase tracking-[0.08em] text-ui-base-muted leading-tight",
                "{eyebrow}"
            }
            h2 {
                id: "{title_id}",
                class: "text-[15px] font-bold tracking-[-0.02em] text-ui-base-content mt-1",
                "{title}"
            }
            button {
                r#type: "button",
                class: "absolute top-2.5 right-2.5 size-6 inline-flex items-center justify-center rounded-ui-sm bg-transparent text-ui-base-muted hover:bg-ui-surface-hover hover:text-ui-base-content focus-visible:outline-none focus-visible:shadow-[var(--ui-focus-ring)] transition-colors cursor-pointer",
                "aria-label": "Close (Esc)",
                "data-overlay": "{overlay_id}",
                span { class: "icon-[lucide--x] size-4", "aria-hidden": "true" }
            }
        }
    }
}

/// Notification-source banner: a 26×26 white icon tile on the left, with an
/// eyebrow label and a single-line truncated title to the right.
///
/// The `tile` slot accepts the leading icon element (typically a
/// `NotificationIcon`).
#[component]
pub fn ModalSourceRow(eyebrow: String, title: String, tile: Element) -> Element {
    rsx! {
        div { class: "flex items-center gap-2.5 px-4 py-2.5 bg-ui-base-200 border-b border-ui-border-light",
            div { class: "flex items-center justify-center shrink-0 size-[26px] rounded-ui-sm bg-ui-surface border border-ui-border text-ui-base-content overflow-hidden [&>*]:size-4 [&>*]:text-[16px]",
                { tile }
            }
            div { class: "flex flex-col min-w-0 leading-[1.25]",
                div { class: "text-[10px] font-semibold uppercase tracking-[0.08em] text-ui-base-muted leading-tight",
                    "{eyebrow}"
                }
                div { class: "text-[12.5px] font-semibold text-ui-base-content truncate",
                    "{title}"
                }
            }
        }
    }
}

/// Modal footer with a left-aligned keyboard-hint strip and right-aligned
/// action buttons. Uses a subtle vertical gradient
/// (`bg-ui-surface` → `bg-ui-base-200`).
///
/// `hint` is the Tab/Esc kbd row; `children` are the action buttons (typically
/// a `Ghost`-variant Cancel and a `Primary`-variant submit).
#[component]
pub fn ModalFooter(hint: Element, children: Element) -> Element {
    rsx! {
        footer { class: "flex items-center justify-between gap-3 px-4 pt-3 pb-3.5 border-t border-ui-border-light bg-gradient-to-b from-ui-surface to-ui-base-200 rounded-b-ui-lg",
            div { class: "inline-flex items-center gap-1.5 text-[10px] text-ui-base-muted",
                { hint }
            }
            div { class: "inline-flex items-center gap-2",
                { children }
            }
        }
    }
}
