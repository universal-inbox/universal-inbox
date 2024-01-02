#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_router::prelude::use_navigator;
use fermi::use_atom_ref;

use universal_inbox::user::{EmailValidationToken, UserId};

use crate::{
    components::spinner::Spinner,
    model::{AuthenticationState, UI_MODEL},
    route::Route,
    services::user_service::{UserCommand, CONNECTED_USER},
};

#[inline_props]
#[allow(unused_variables)]
pub fn EmailVerificationPage(
    cx: Scope,
    user_id: UserId,
    email_validation_token: EmailValidationToken,
) -> Element {
    let connected_user_ref = use_atom_ref(cx, &CONNECTED_USER);
    let ui_model_ref = use_atom_ref(cx, &UI_MODEL);
    let user_service = use_coroutine_handle::<UserCommand>(cx).unwrap();
    let nav = use_navigator(cx);
    let authentication_state = ui_model_ref.read().authentication_state.clone();

    use_future(cx, (), |()| {
        to_owned![user_id];
        to_owned![email_validation_token];
        to_owned![user_service];

        async move {
            user_service.send(UserCommand::VerifyEmail(user_id, email_validation_token));
        }
    });

    if ui_model_ref.read().authentication_state == AuthenticationState::NotAuthenticated {
        nav.push(Route::LoginPage {});
        cx.needs_update();
        None
    } else if connected_user_ref
        .read()
        .as_ref()
        .map(|user| user.is_email_validated())
        .unwrap_or_default()
    {
        nav.push(Route::NotificationsPage {});
        cx.needs_update();
        None
    } else {
        render! {
            div {
                class: "h-full flex justify-center items-center",

                Spinner {}
                "Validating email verification..."
            }
        }
    }
}
