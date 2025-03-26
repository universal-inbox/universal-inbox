#![allow(non_snake_case)]

use dioxus::prelude::*;

pub fn UniversalInboxTitle() -> Element {
    rsx! {
        span {
            class: "font-extrabold text-transparent bg-clip-text bg-linear-to-b from-[#12B1FA] to-primary",
            "Universal Inbox"
        }
    }
}
