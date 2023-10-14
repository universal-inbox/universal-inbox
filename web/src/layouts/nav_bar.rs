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
        NavBar {}
        Outlet::<Route> {}
        Footer {}
        ToastZone {}
    }
}
