#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsQuestionCircle, Icon};
use dioxus_router::prelude::*;
use fermi::use_atom_ref;

use crate::{
    route::Route, services::user_service::UserCommand, services::user_service::CONNECTED_USER,
};

#[component]
pub fn EmailValidatedLayout(cx: Scope) -> Element {
    let connected_user_ref = use_atom_ref(cx, &CONNECTED_USER);
    let user_service = use_coroutine_handle::<UserCommand>(cx).unwrap();

    if connected_user_ref
        .read()
        .as_ref()
        .map(|user| user.is_email_validated())
        .unwrap_or_default()
    {
        render! { Outlet::<Route> {} }
    } else {
        render! {
            div {
                class: "alert alert-info text-sm",
                Icon { class: "w-5 h-5", icon: BsQuestionCircle }
                "Please check your emails for a verification link"
            }

            form {
                class: "flex flex-col justify-center gap-4 px-10 pb-8",
                onsubmit: |_| user_service.send(UserCommand::ResendVerificationEmail),

                button {
                    class: "btn btn-primary",
                    r#type: "submit",
                    "Send a new email verification link"
                }
            }
        }
    }
}
