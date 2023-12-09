#![allow(non_snake_case)]

use dioxus::prelude::*;

pub fn UniversalInboxTitle(cx: Scope) -> Element {
    render! {
        span {
            class: "font-extrabold text-transparent bg-clip-text bg-gradient-to-b from-[#12B1FA] to-primary",
            "Universal Inbox"
        }
    }
}
