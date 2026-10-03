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

/// Id of the element wrapping the latest read message of a collapsible
/// thread. The preview scrolls it to the top when a thread is opened.
pub const LATEST_READ_ANCHOR_ID: &str = "thread-latest-read";
const SCROLL_ANCHOR_OFFSET_PX: f64 = 12.0;
// Icon + text inside a `ThreadDivider` pill. `align-top` keeps the inline-flex
// box off the label's text baseline so the content stays vertically centered.
const DIVIDER_CONTENT_CLASS: &str = "inline-flex items-center gap-1 align-top";

/// Collapse state of a thread whose already-read messages are hidden.
#[derive(Clone, Copy)]
pub struct ThreadCollapse {
    /// Show every read message instead of only the latest one.
    pub show_all: Signal<bool>,
    /// Show the thread start (root message) even though it was read.
    pub show_root: Signal<bool>,
}

/// Collapse state shared by thread previews hiding already-read messages
/// (Slack threads, GitHub discussions).
///
/// `thread_state` returns a key identifying the displayed thread and whether
/// it has read messages. When the key changes, the collapse state is reset to
/// `expand_details` and, if some messages are read, the preview container
/// (`container_id`) is scrolled so that [`LATEST_READ_ANCHOR_ID`] sits at the
/// top, keeping the unread messages in view.
pub fn use_thread_collapse(
    container_id: &'static str,
    expand_details: ReadSignal<bool>,
    thread_state: impl Fn() -> (String, bool) + 'static,
) -> ThreadCollapse {
    let mut show_all = use_signal(|| false);
    let mut show_root = use_signal(|| false);
    let _resource = use_resource(move || async move {
        let expanded = expand_details();
        *show_all.write() = expanded;
        *show_root.write() = expanded;
    });

    let mut shown_thread_key = use_signal(|| None::<String>);
    use_effect(move || {
        let (key, has_read_messages) = thread_state();
        if shown_thread_key.peek().as_ref() == Some(&key) {
            return;
        }
        shown_thread_key.set(Some(key));
        let expanded = *expand_details.peek();
        show_all.set(expanded);
        show_root.set(expanded);
        if has_read_messages {
            spawn(async move {
                gloo_timers::future::TimeoutFuture::new(0).await;
                let _ = crate::utils::scroll_child_to_top(
                    container_id,
                    LATEST_READ_ANCHOR_ID,
                    SCROLL_ANCHOR_OFFSET_PX,
                );
            });
        }
    });

    ThreadCollapse {
        show_all,
        show_root,
    }
}

/// Button pill revealing hidden (already read) messages.
#[component]
pub fn ExpandDivider(
    icon: &'static str,
    label: String,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        ThreadDivider {
            button {
                r#type: "button",
                class: DIVIDER_CONTENT_CLASS,
                onclick: move |evt| onclick.call(evt),
                span { class: "{icon} size-3" }
                "{label}"
            }
        }
    }
}

/// Accent pill placed above the first unread message.
#[component]
pub fn UnreadDivider(label: String) -> Element {
    rsx! {
        ThreadDivider {
            unread: true,
            span {
                class: DIVIDER_CONTENT_CLASS,
                span { class: "icon-[lucide--arrow-down] size-3" }
                "{label}"
            }
        }
    }
}

/// "Show thread start" control replacing a read root message.
#[component]
pub fn ShowThreadStartDivider(onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        ExpandDivider {
            icon: "icon-[lucide--arrow-up-to-line]",
            label: "Show thread start",
            onclick,
        }
    }
}

/// "N earlier replies…" control replacing hidden read messages.
#[component]
pub fn EarlierMessagesDivider(
    hidden_count: usize,
    singular: &'static str,
    plural: &'static str,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let noun = if hidden_count == 1 { singular } else { plural };
    rsx! {
        ExpandDivider {
            icon: "icon-[lucide--arrow-up]",
            label: format!("{hidden_count} earlier {noun}…"),
            onclick,
        }
    }
}

/// "N NEW REPLIES" divider placed above the first unread message.
#[component]
pub fn NewMessagesDivider(
    unread_count: usize,
    singular: &'static str,
    plural: &'static str,
) -> Element {
    let noun = if unread_count == 1 { singular } else { plural }.to_uppercase();
    rsx! {
        UnreadDivider { label: format!("{unread_count} NEW {noun}") }
    }
}
