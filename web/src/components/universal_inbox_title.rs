#![allow(non_snake_case)]

use dioxus::prelude::*;

use crate::icons::UILogo;

pub fn UniversalInboxTitle() -> Element {
    rsx! {
        span {
            class: "font-extrabold text-transparent bg-clip-text bg-linear-to-b from-[#12B1FA] to-primary",
            "Universal Inbox"
        }
    }
}

/// Vertical hero lockup used at the top of every auth screen — the
/// 64×78 logo mark stacked above the gradient "Universal Inbox" wordmark.
///
/// The `brand-word` class remains custom CSS (gradient `background-clip: text`)
/// and the `brand-mark` class anchors the `.brand-lockup.hero .brand-mark`
/// drop-shadow rule.
#[component]
pub fn BrandHeroLockup() -> Element {
    rsx! {
        div { class: "flex flex-col items-center mb-5",
            div { class: "brand-lockup hero flex flex-col items-center gap-1",
                UILogo { class: "brand-mark" }
                span { class: "brand-word", "Universal Inbox" }
            }
        }
    }
}
