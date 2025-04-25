#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus::web::WebEventExt;

use crate::services::flyonui::{forget_flyonui_collapse_element, init_flyonui_collapse_element};

#[component]
pub fn Collapse(
    id: String,
    opened: ReadOnlySignal<Option<bool>>,
    header: Element,
    children: Element,
) -> Element {
    let (collapse_toggle_style, collapse_content_style) = use_memo(move || {
        if opened().unwrap_or_default() {
            ("open", "")
        } else {
            ("", "hidden")
        }
    })();
    let mut mounted_element: Signal<Option<web_sys::Element>> = use_signal(|| None);
    use_drop(move || {
        if let Some(element) = mounted_element() {
            forget_flyonui_collapse_element(&element);
        }
    });

    rsx! {
        button {
            id: "collapse-toggle-{id}",
            onmounted: move |element| {
                let web_element = element.as_web_event();
                init_flyonui_collapse_element(&web_element);
                mounted_element.set(Some(web_element));
            },
            class: "collapse-toggle flex items-center gap-2 p-2 w-full cursor-pointer {collapse_toggle_style}",
            "data-collapse": "#collapse-content-{id}",
            "type": "button",

            { header }

            span { class: "icon-[tabler--chevron-down] collapse-open:rotate-180 ms-2 size-4" }
        }

        div {
            id: "collapse-content-{id}",
            class: "collapse w-full overflow-hidden transition-[height] duration-300 p-2 flex flex-col gap-2 text-sm {collapse_content_style}",

            { children }
        }
    }
}
