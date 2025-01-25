use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(feature = "trunk")] {
        pub const GOOGLE_LOGO: &str = "/images/google-logo.svg";
    } else {
        use dioxus::prelude::*;
        pub const GOOGLE_LOGO: Asset = asset!("/images/google-logo.svg");
    }
}
