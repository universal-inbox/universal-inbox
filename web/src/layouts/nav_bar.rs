#![allow(non_snake_case)]

use dioxus::prelude::*;

use crate::{
    components::{footer::Footer, nav_bar::NavBar, toast_zone::ToastZone},
    model::VERSION_MISMATCH,
    route::Route,
};

#[component]
pub fn NavBarLayout() -> Element {
    let version_mismatch = VERSION_MISMATCH.read();

    rsx! {
        div {
            class: "h-full flex flex-col relative text-sm",
            NavBar {}
            if let Some(ref backend_version) = *version_mismatch {
                div {
                    class: "w-full bg-warning text-warning-content px-4 py-2 text-center text-sm flex items-center justify-center gap-2",
                    span {
                        "A new version ({backend_version}) is available but could not be loaded automatically. Please hard-refresh your browser (Ctrl+Shift+R / Cmd+Shift+R) or clear your cache."
                    }
                }
            }
            div {
                class: "w-full flex-1 overflow-hidden",
                Outlet::<Route> {}
            }
            Footer {}
            ToastZone {}
        }
    }
}
