#![allow(non_snake_case)]

use dioxus::prelude::*;

#[inline_props]
#[allow(unused_variables)]
pub fn PageNotFound(cx: Scope, route: Vec<String>) -> Element {
    render! {
        div {
            class: "h-full flex justify-center items-center",
            h1 { "Page not found" }
        }
    }
}
