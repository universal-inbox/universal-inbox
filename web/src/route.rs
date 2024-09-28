use dioxus::prelude::*;

use universal_inbox::user::{EmailValidationToken, PasswordResetToken, UserId};

use crate::{
    auth::AuthPage,
    layouts::{
        authenticated::AuthenticatedLayout, email_validated::EmailValidatedLayout,
        fullpage::FullpageLayout, nav_bar::NavBarLayout,
    },
    pages::{
        email_verification_page::EmailVerificationPage, login_page::LoginPage,
        notifications_page::NotificationsPage, page_not_found::PageNotFound,
        password_reset_page::PasswordResetPage, password_update_page::PasswordUpdatePage,
        settings_page::SettingsPage, signup_page::SignupPage, synced_tasks_page::SyncedTasksPage,
        user_profile_page::UserProfilePage,
    },
};

#[derive(Routable, Clone, Debug, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[layout(FullpageLayout)]
      #[route("/users/:user_id/email-verification/:email_validation_token")]
      EmailVerificationPage { user_id: UserId, email_validation_token: EmailValidationToken },
      #[route("/users/:user_id/password-reset/:password_reset_token")]
      PasswordUpdatePage { user_id: UserId, password_reset_token: PasswordResetToken },
    #[end_layout]
    #[layout(AuthenticatedLayout)]
      #[layout(FullpageLayout)]
        #[route("/login")]
        LoginPage {},
        #[route("/signup")]
        SignupPage {},
        #[route("/password-reset")]
        PasswordResetPage {},
      #[end_layout]
      #[route("/auth-oidc-callback?:query")]
      AuthPage { query: String },
      #[layout(EmailValidatedLayout)]
        #[layout(NavBarLayout)]
          #[route("/")]
          NotificationsPage {},
          #[route("/synced-tasks")]
          SyncedTasksPage {},
          #[route("/settings")]
          SettingsPage {},
          #[route("/profile")]
          UserProfilePage {},
        #[end_layout]
      #[end_layout]
    #[end_layout]
    #[route("/:..route")]
    PageNotFound {
        route: Vec<String>
    },
}
