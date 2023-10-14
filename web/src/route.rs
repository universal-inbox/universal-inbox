use dioxus::prelude::*;
use dioxus_router::prelude::*;

use crate::{
    auth::AuthPage,
    layouts::{authenticated::AuthenticatedLayout, nav_bar::NavBarLayout},
    pages::{
        notifications_page::NotificationsPage, page_not_found::PageNotFound,
        settings_page::SettingsPage,
    },
};

#[derive(Routable, Clone, Debug, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[layout(AuthenticatedLayout)]
    #[layout(NavBarLayout)]
    #[route("/auth-oidc-callback?:query")]
    AuthPage { query: String },
    #[route("/")]
    NotificationsPage {},
    #[route("/settings")]
    SettingsPage {},
    #[end_layout]
    #[end_layout]
    #[route("/:..route")]
    PageNotFound {
        route: Vec<String>
    },
}
