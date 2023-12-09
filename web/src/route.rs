use dioxus::prelude::*;
use dioxus_router::prelude::*;

use crate::{
    auth::AuthPage,
    layouts::{authenticated::AuthenticatedLayout, nav_bar::NavBarLayout},
    pages::{
        login_page::LoginPage, notifications_page::NotificationsPage, page_not_found::PageNotFound,
        recover_password_page::RecoverPasswordPage, settings_page::SettingsPage,
        signup_page::SignupPage,
    },
};

#[derive(Routable, Clone, Debug, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[layout(AuthenticatedLayout)]
    #[route("/login")]
    LoginPage {},
    #[route("/signup")]
    SignupPage {},
    #[route("/recover-password")]
    RecoverPasswordPage {},
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
