#![allow(non_snake_case)]

use cfg_if::cfg_if;
use dioxus::prelude::*;

use crate::images::UI_LOGO_SYMBOL_TRANSPARENT;

cfg_if! {
    if #[cfg(feature = "trunk")] {
        pub const GOOGLE_LOGO: &str = "/images/google-logo.svg";
    } else {
        pub const GOOGLE_LOGO: Asset = asset!("/images/google-logo.svg");
    }
}

#[component]
pub fn UILogo(class: String, alt: Option<String>) -> Element {
    let alt = alt.unwrap_or_else(|| "Universal Inbox logo".to_string());

    rsx! {
        img {
            class,
            src: "{UI_LOGO_SYMBOL_TRANSPARENT}",
            alt,
        }
    }
}
