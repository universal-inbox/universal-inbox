#![allow(non_snake_case)]

use dioxus::prelude::dioxus_core::needs_update;
use dioxus::prelude::*;

use crate::{
    components::{auth_widgets::PrimaryBtn, ui::PageHeader},
    route::Route,
    services::user_service::{CONNECTED_USER, UserCommand},
};

#[component]
pub fn EmailValidationRequiredPage() -> Element {
    let user_service = use_coroutine_handle::<UserCommand>();
    let nav = use_navigator();

    let connected_user = CONNECTED_USER.read();
    let Some(user) = connected_user.as_ref() else {
        // Not logged in: send the visitor to the login page.
        nav.push(Route::LoginPage {});
        needs_update();
        return rsx! {};
    };

    if user.is_email_validated() {
        // Email already verified (or an auth method that does not require it):
        // there is nothing to gate, go to the app.
        nav.push(Route::NotificationsPage {});
        needs_update();
        return rsx! {};
    }

    let email = user
        .email
        .as_ref()
        .map(|email| email.to_string())
        .unwrap_or_default();

    rsx! {
        PageHeader {
            title: "Verify your email".to_string(),
            subtitle: Some("One last step before you can use Universal Inbox.".to_string()),
        }
        p { class: "text-sm text-ui-base-muted leading-normal mb-7",
            "We sent a verification link to "
            span { class: "font-semibold text-ui-base-content", "{email}" }
            ". Click it to activate your inbox, then come back here."
        }

        PrimaryBtn {
            onclick: move |_| {
                user_service.send(UserCommand::ResendVerificationEmail);
            },
            "Resend verification email"
        }
    }
}
