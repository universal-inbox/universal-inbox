#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsQuestionCircle, Icon};

use crate::{
    route::Route, services::user_service::UserCommand, services::user_service::CONNECTED_USER,
};

#[component]
pub fn EmailValidatedLayout() -> Element {
    let user_service = use_coroutine_handle::<UserCommand>();

    if CONNECTED_USER
        .read()
        .as_ref()
        .map(|user| user.is_email_validated())
        .unwrap_or_default()
    {
        rsx! { Outlet::<Route> {} }
    } else {
        rsx! {
            div {
                class: "alert alert-info text-sm",
                Icon { class: "w-5 h-5", icon: BsQuestionCircle }
                "Please check your emails for a verification link"
            }

            form {
                class: "flex flex-col justify-center gap-4 px-10 pb-8",
                onsubmit: move |_| user_service.send(UserCommand::ResendVerificationEmail),

                button {
                    class: "btn btn-primary",
                    r#type: "submit",
                    "Send a new email verification link"
                }
            }
        }
    }
}
