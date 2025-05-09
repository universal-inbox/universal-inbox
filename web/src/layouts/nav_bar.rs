#![allow(non_snake_case)]

use dioxus::prelude::*;

use crate::{
    components::{footer::Footer, nav_bar::NavBar, toast_zone::ToastZone},
    route::Route,
};

#[component]
pub fn NavBarLayout() -> Element {
    rsx! {
        div {
            class: "h-full flex flex-col relative text-sm",
            NavBar {}
            div {
                class: "w-full flex-1 overflow-hidden",
                Outlet::<Route> {}
            }
            Footer {}
            ToastZone {}
        }
    }
}
