#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{BsBoxArrowInLeft, BsCheckCircle, BsQuestionCircle},
    Icon,
};
use dioxus_router::prelude::*;
use fermi::use_atom_ref;

use crate::{
    model::UI_MODEL, route::Route, services::user_service::UserCommand,
    services::user_service::CONNECTED_USER,
};

#[component]
pub fn EmailValidatedLayout(cx: Scope) -> Element {
    let connected_user_ref = use_atom_ref(cx, &CONNECTED_USER);
    let ui_model_ref = use_atom_ref(cx, &UI_MODEL);
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
                class: "flex min-h-screen items-center justify-center bg-base-100 relative",

                button {
                    class: "btn btn-ghost absolute top-4 right-4",
                    "data-tip": "Logout",
                    onclick: |_| user_service.send(UserCommand::Logout),
                    Icon { class: "w-5 h-5", icon: BsBoxArrowInLeft }
                }

                div {
                    class: "m-4 min-h-[50vh] w-full max-w-md",

                    main {
                        div {
                            class: "flex flex-col items-center justify-center p-8",
                            img {
                                class: "rounded-full w-48 h-48",
                                src: "images/ui-logo-transparent.png",
                                alt: "Universal Inbox logo",
                            }
                        }

                        form {
                            class: "flex flex-col justify-center gap-4 px-10 pb-8",
                            onsubmit: |_| user_service.send(UserCommand::ResendVerificationEmail),

                            if let Some(confirmation_message) = &ui_model_ref.read().confirmation_message {
                                render! {
                                    div {
                                        class: "alert alert-success text-sm",
                                        Icon { class: "w-5 h-5", icon: BsCheckCircle }
                                        "{confirmation_message}"
                                    }
                                }
                            } else {
                                render! {
                                    div {
                                        class: "alert alert-info text-sm",
                                        Icon { class: "w-5 h-5", icon: BsQuestionCircle }
                                        "Please check your emails for a verification link"
                                    }
                                }
                            }

                            if let Some(error_message) = &ui_model_ref.read().error_message {
                                render! {
                                    div { class: "alert alert-error text-sm", "{error_message}" }
                                }
                            }

                            button {
                                class: "btn btn-primary",
                                r#type: "submit",
                                "Send a new email verification link"
                            }
                        }
                    }
                }
            }
        }
    }
}
