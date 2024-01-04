#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{BsBoxArrowInLeft, BsCheckCircle},
    Icon,
};
use dioxus_router::prelude::*;
use fermi::use_atom_ref;

use crate::{
    model::UI_MODEL,
    route::Route,
    services::user_service::{UserCommand, CONNECTED_USER},
};

#[component]
pub fn FullpageLayout(cx: Scope) -> Element {
    let ui_model_ref = use_atom_ref(cx, &UI_MODEL);
    let connected_user_ref = use_atom_ref(cx, &CONNECTED_USER);
    let user_service = use_coroutine_handle::<UserCommand>(cx).unwrap();

    render! {
        div {
            class: "flex min-h-screen items-center justify-center bg-base-100 relative",

            if connected_user_ref.read().is_some() {
                render! {
                    button {
                        class: "btn btn-ghost absolute top-4 right-4",
                        "data-tip": "Logout",
                        onclick: |_| user_service.send(UserCommand::Logout),
                        Icon { class: "w-5 h-5", icon: BsBoxArrowInLeft }
                    }
                }
            }

            div {
                class: "m-4 min-h-[50vh] w-full max-w-md",

                main {
                    div {
                        class: "flex flex-col items-center justify-center",
                        img {
                            class: "rounded-full w-48 h-48",
                            src: "/images/ui-logo-transparent.png",
                            alt: "Universal Inbox logo",
                        }
                    }

                    if let Some(error_message) = &ui_model_ref.read().error_message {
                        render! {
                            div { class: "alert alert-error text-sm", "{error_message}" }
                        }
                    }

                    if let Some(confirmation_message) = &ui_model_ref.read().confirmation_message {
                        render! {
                            div {
                                class: "flex flex-col items-center justify-center",

                                div {
                                    class: "alert alert-success text-sm",
                                    Icon { class: "w-5 h-5", icon: BsCheckCircle }
                                    "{confirmation_message}"
                                }

                                Link {
                                    class: "btn btn-primary mt-2",
                                    to: Route::LoginPage {},
                                    "Return to Universal Inbox"
                                }
                            }
                        }
                    } else {
                        render! {
                            Outlet::<Route> {}
                        }
                    }
                }
            }
        }
    }
}
