use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(feature = "trunk")] {
        pub const UI_LOGO_SYMBOL_TRANSPARENT: &str = "/images/ui-logo-symbol-transparent.svg";
    } else {
        use dioxus::prelude::*;

        pub const UI_LOGO_SYMBOL_TRANSPARENT: Asset = asset!("/images/ui-logo-symbol-transparent.svg");
    }
}
