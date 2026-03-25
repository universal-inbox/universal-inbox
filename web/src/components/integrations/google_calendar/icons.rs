#![allow(non_snake_case)]

use dioxus::prelude::*;

#[component]
pub fn GoogleCalendar(class: Option<String>) -> Element {
    let class = class.unwrap_or_default();
    rsx! { span { class: "icon-[logos--google-calendar] {class}" } }
}
