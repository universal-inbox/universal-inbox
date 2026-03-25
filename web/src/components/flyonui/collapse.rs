#![allow(non_snake_case)]

use dioxus::prelude::*;

#[component]
pub fn Collapse(
    id: String,
    opened: ReadSignal<Option<bool>>,
    header: Element,
    children: Element,
) -> Element {
    let mut is_open = use_signal(move || opened().unwrap_or(false));

    use_effect(move || {
        if let Some(val) = opened() {
            is_open.set(val);
        }
    });

    rsx! {
        div {
            class: "collapse-panel w-full",

            div {
                class: "collapse-toggle flex items-center gap-2 p-2 w-full cursor-pointer",
                onclick: move |_| is_open.toggle(),

                { header }

                span {
                    class: if is_open() {
                        "icon-[tabler--chevron-down] rotate-180 ms-2 size-4 transition-transform duration-200"
                    } else {
                        "icon-[tabler--chevron-down] ms-2 size-4 transition-transform duration-200"
                    }
                }
            }

            if is_open() {
                div {
                    class: "collapse w-full overflow-hidden p-2 flex flex-col gap-2 text-sm",
                    { children }
                }
            }
        }
    }
}
