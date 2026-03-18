use dioxus::prelude::*;

use universal_inbox::{
    notification::NotificationId,
    task::TaskId,
    user::{EmailValidationToken, PasswordResetToken, UserId},
};

use crate::{
    auth::AuthPage,
    layouts::{
        authenticated::AuthenticatedLayout, fullpage::FullpageLayout, nav_bar::NavBarLayout,
    },
    pages::{
        email_verification_page::EmailVerificationPage,
        login_page::LoginPage,
        notifications_page::{NotificationPage, NotificationsPage},
        page_not_found::PageNotFound,
        passkey_login_page::PasskeyLoginPage,
        passkey_signup_page::PasskeySignupPage,
        password_reset_page::PasswordResetPage,
        password_update_page::PasswordUpdatePage,
        settings_page::SettingsPage,
        signup_page::SignupPage,
        subscription_settings_page::SubscriptionSettingsPage,
        synced_tasks_page::{SyncedTaskPage, SyncedTasksPage},
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
        #[route("/passkey-login")]
        PasskeyLoginPage {},
        #[route("/passkey-signup")]
        PasskeySignupPage {},
      #[end_layout]
      #[route("/auth-oidc-callback?:query")]
      AuthPage { query: String },
      #[layout(NavBarLayout)]
        #[route("/")]
        NotificationsPage {},
        #[route("/notifications/:notification_id")]
        NotificationPage { notification_id: NotificationId },
        #[route("/synced-tasks")]
        SyncedTasksPage {},
        #[route("/synced-task/:task_id")]
        SyncedTaskPage { task_id: TaskId },
        #[route("/settings")]
        SettingsPage {},
        #[route("/subscription")]
        SubscriptionSettingsPage {},
        #[route("/profile")]
        UserProfilePage {},
      #[end_layout]
    #[end_layout]
    #[route("/:..route")]
    PageNotFound {
        route: Vec<String>
    },
}
