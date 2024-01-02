use dioxus::prelude::*;
use dioxus_router::prelude::*;
use universal_inbox::user::{EmailValidationToken, UserId};

use crate::{
    auth::AuthPage,
    layouts::{
        authenticated::AuthenticatedLayout, email_validated::EmailValidatedLayout,
        nav_bar::NavBarLayout,
    },
    pages::{
        email_verification_page::EmailVerificationPage, login_page::LoginPage,
        notifications_page::NotificationsPage, page_not_found::PageNotFound,
        recover_password_page::RecoverPasswordPage, settings_page::SettingsPage,
        signup_page::SignupPage,
    },
};

#[derive(Routable, Clone, Debug, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[route("/users/:user_id/email_verification/:email_validation_token")]
    EmailVerificationPage { user_id: UserId, email_validation_token: EmailValidationToken },
    #[layout(AuthenticatedLayout)]
      #[route("/login")]
      LoginPage {},
      #[route("/signup")]
      SignupPage {},
      #[route("/recover-password")]
      RecoverPasswordPage {},
      #[route("/auth-oidc-callback?:query")]
      AuthPage { query: String },
      #[layout(EmailValidatedLayout)]
        #[layout(NavBarLayout)]
          #[route("/")]
          NotificationsPage {},
          #[route("/settings")]
          SettingsPage {},
        #[end_layout]
      #[end_layout]
    #[end_layout]
    #[route("/:..route")]
    PageNotFound {
        route: Vec<String>
    },
}
