#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_router::prelude::*;

use crate::{
    components::{footer::Footer, nav_bar::NavBar, toast_zone::ToastZone},
    route::Route,
};

#[inline_props]
pub fn NavBarLayout(cx: Scope) -> Element {
    render! {
        div {
            class: "h-full flex flex-col relative text-sm h-16",
            NavBar {}
            div {
                class: "w-full h-full flex-1 absolute top-0 pt-16 pb-11",
                Outlet::<Route> {}
            }
            div {
                class: "w-full absolute bottom-1 h-10",
                Footer {}
            }
            ToastZone {}
        }
    }
}
