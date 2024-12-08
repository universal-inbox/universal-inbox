#![allow(non_snake_case)]

use dioxus::prelude::*;

#[component]
pub fn Spinner(class: Option<String>) -> Element {
    let class = class.unwrap_or_default();

    rsx! {
        div {
            role: "status",

            span { class: "loading loading-ring loading-lg text-primary {class}" }
            span { class: "sr-only", "Loading..." }
        }
    }
}
