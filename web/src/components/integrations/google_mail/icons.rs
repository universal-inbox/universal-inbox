#![allow(non_snake_case)]

use dioxus::prelude::*;

#[component]
pub fn GoogleMail(class: Option<String>) -> Element {
    let class = class.unwrap_or_default();
    rsx! { span { class: "icon-[logos--google-gmail] {class}" } }
}

#[component]
pub fn Mail(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: class.unwrap_or_default(),
            "viewBox": "0 0 512 512",
            fill: "currentColor",
            stroke: "currentColor",
            rect {
                height: "320",
                rx: "40",
                ry: "40",
                style: "fill:none;stroke-linecap:round;stroke-linejoin:round;stroke-width:32px",
                width: "416",
                x: "48",
                y: "96",
            }
            polyline {
                points: "112 160 256 272 400 160",
                style: "fill:none;stroke-linecap:round;stroke-linejoin:round;stroke-width:32px",
            }
        }
    }
}
