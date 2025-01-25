#![allow(non_snake_case)]

use dioxus::prelude::*;

use crate::components::spinner::Spinner;

#[component]
#[allow(unused_variables)]
pub fn Loading(label: ReadOnlySignal<String>) -> Element {
    rsx! {
        div {
            class: "h-full flex justify-center items-center overflow-hidden",

            Spinner {}
            "{label()}"
        }
    }
}
