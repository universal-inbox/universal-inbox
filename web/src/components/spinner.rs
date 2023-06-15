use dioxus::prelude::*;

pub fn spinner(cx: Scope) -> Element {
    cx.render(rsx!(
        div {
            role: "status",

            span { class: "loading loading-ring loading-lg text-secondary" }
            span { class: "sr-only", "Loading..." }
        }
    ))
}
