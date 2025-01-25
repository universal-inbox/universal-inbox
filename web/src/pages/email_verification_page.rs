#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_router::prelude::use_navigator;

use universal_inbox::user::{EmailValidationToken, UserId};

use crate::{
    components::loading::Loading,
    model::{AuthenticationState, UI_MODEL},
    route::Route,
    services::user_service::{UserCommand, CONNECTED_USER},
};

#[component]
pub fn EmailVerificationPage(
    user_id: UserId,
    email_validation_token: EmailValidationToken,
) -> Element {
    let user_service = use_coroutine_handle::<UserCommand>();
    let nav = use_navigator();

    let _ = use_resource(move || {
        to_owned![user_id];
        to_owned![email_validation_token];
        to_owned![user_service];

        async move {
            user_service.send(UserCommand::VerifyEmail(user_id, email_validation_token));
        }
    });

    if UI_MODEL.read().authentication_state == AuthenticationState::NotAuthenticated {
        nav.push(Route::LoginPage {});
        needs_update();
        rsx! {}
    } else if CONNECTED_USER
        .read()
        .as_ref()
        .map(|user| user.is_email_validated())
        .unwrap_or_default()
    {
        nav.push(Route::NotificationsPage {});
        needs_update();
        rsx! {}
    } else {
        rsx! { Loading { label: "Validating email verification..." } }
    }
}
