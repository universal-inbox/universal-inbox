#![allow(non_snake_case)]

use dioxus::prelude::*;

/// Two-column key/value grid used by every notification preview pane for metadata.
///
/// Renders SMALL-CAPS labels in the left column and body-size values in the
/// right column. Use [`FieldRow`] for each row when the value is a flat
/// sequence of inline children.
#[component]
pub fn FieldGrid(children: Element) -> Element {
    rsx! {
        div {
            class: "grid grid-cols-[92px_1fr] gap-y-[10px] gap-x-[14px] items-center mb-3",
            {children}
        }
    }
}

/// One row in a [`FieldGrid`].
#[component]
pub fn FieldRow(label: String, children: Element) -> Element {
    rsx! {
        span {
            class: "text-[11px] font-bold uppercase tracking-[0.04em] text-ui-base-muted self-center",
            "{label}"
        }
        span {
            class: "text-[12.5px] text-ui-base-content flex items-center gap-1.5 flex-wrap",
            {children}
        }
    }
}
