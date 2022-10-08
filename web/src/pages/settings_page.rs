use dioxus::prelude::*;

pub fn settings_page(cx: Scope) -> Element {
    cx.render(rsx!(
        div { "Settings" }
    ))
}
