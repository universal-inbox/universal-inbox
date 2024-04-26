#![allow(non_snake_case)]

use dioxus::prelude::*;

#[component]
#[allow(unused_variables)]
pub fn PageNotFound(route: Vec<String>) -> Element {
    rsx! {
        div {
            class: "h-full flex justify-center items-center",
            h1 { "Page not found" }
        }
    }
}
