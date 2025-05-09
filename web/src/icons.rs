#![allow(non_snake_case)]

use cfg_if::cfg_if;
use dioxus::prelude::*;

use crate::images::UI_LOGO_SYMBOL_TRANSPARENT;

cfg_if! {
    if #[cfg(feature = "trunk")] {
        pub const GOOGLE_LOGO: &str = "/images/google-logo.svg";
        pub const PASSKEY_LOGO: &str = "/images/passkey-logo.svg";
    } else {
        pub const GOOGLE_LOGO: Asset = asset!("/images/google-logo.svg");
        pub const PASSKEY_LOGO: Asset = asset!("/images/passkey-logo.svg");
    }
}

#[component]
pub fn UILogo(class: String, alt: Option<String>) -> Element {
    let alt = alt.unwrap_or_else(|| "Universal Inbox logo".to_string());

    rsx! {
        img {
            class: "{class} scale-x-270 scale-y-210",
            src: "{UI_LOGO_SYMBOL_TRANSPARENT}",
            alt,
        }
    }
}
