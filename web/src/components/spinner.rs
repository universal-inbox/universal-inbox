#![allow(non_snake_case)]

use dioxus::prelude::*;

#[component]
pub fn Spinner(class: Option<String>) -> Element {
    rsx! {
        div {
            role: "status",

            span { class: "loading loading-ring loading-lg text-primary {class.unwrap_or_default()}" }
            span { class: "sr-only", "Loading..." }
        }
    }
}
