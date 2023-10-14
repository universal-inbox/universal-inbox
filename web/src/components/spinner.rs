#![allow(non_snake_case)]

use dioxus::prelude::*;

pub fn Spinner(cx: Scope) -> Element {
    render! {
        div {
            role: "status",

            span { class: "loading loading-ring loading-lg text-secondary" }
            span { class: "sr-only", "Loading..." }
        }
    }
}
