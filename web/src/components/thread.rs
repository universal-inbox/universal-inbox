#![allow(non_snake_case)]

use dioxus::prelude::*;

/// Vertical container for a flat or threaded list of messages/comments.
///
/// Pair with [`ThreadChildren`] to indent nested replies. Use [`ThreadDivider`]
/// for "X hidden messages" / "X new unread replies" markers — preserve the exact
/// divider strings when migrating per-kind implementations to this component.
#[component]
pub fn Thread(children: Element) -> Element {
    rsx! {
        div { class: "ui-thread", {children} }
    }
}

/// One message/comment row within a [`Thread`]. Holds its own author header,
/// body, and (optionally) a [`ThreadChildren`] block for nested replies.
#[component]
pub fn ThreadItem(children: Element) -> Element {
    rsx! {
        div { class: "ui-thread-item", {children} }
    }
}

/// Indented nested-reply block. Renders with a left connector rule.
#[component]
pub fn ThreadChildren(children: Element) -> Element {
    rsx! {
        div { class: "ui-thread-children", {children} }
    }
}

/// Section divider used between message groups — typically for the
/// "X hidden messages" / "X new unread replies" affordances. Pass `unread:
/// true` to render in the primary accent (matches what Drive and Gmail
/// already do for the unread divider).
///
/// Renders as a primary-blue pill centered on a faded horizontal rule that
/// stretches across the full container width.
#[component]
pub fn ThreadDivider(unread: Option<bool>, children: Element) -> Element {
    let unread_class = if unread.unwrap_or_default() {
        "unread"
    } else {
        ""
    };
    rsx! {
        div {
            class: "ui-thread-divider {unread_class}",
            span { class: "ui-thread-divider-label", {children} }
        }
    }
}
