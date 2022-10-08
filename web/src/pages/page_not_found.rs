use dioxus::prelude::*;

pub fn page_not_found(cx: Scope) -> Element {
    cx.render(rsx!(
        div { "Not Found" }
    ))
}
