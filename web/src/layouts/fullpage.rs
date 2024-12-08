#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{BsBoxArrowInLeft, BsCheckCircle},
    Icon,
};

use crate::{
    images::UI_LOGO_TRANSPARENT,
    model::UI_MODEL,
    route::Route,
    services::user_service::{UserCommand, CONNECTED_USER},
};

#[component]
pub fn FullpageLayout() -> Element {
    let user_service = use_coroutine_handle::<UserCommand>();

    rsx! {
        div {
            class: "flex min-h-screen items-center justify-center bg-base-100 relative",

            if CONNECTED_USER.read().is_some() {
                button {
                    class: "btn btn-ghost absolute top-4 right-4",
                    "data-tip": "Logout",
                    onclick: move |_| user_service.send(UserCommand::Logout),
                    Icon { class: "w-5 h-5", icon: BsBoxArrowInLeft }
                }
            }

            div {
                class: "m-4 min-h-[50vh] w-full max-w-md",

                main {
                    div {
                        class: "flex flex-col items-center justify-center",
                        img {
                            class: "rounded-full w-48 h-48",
                            src: "{UI_LOGO_TRANSPARENT}",
                            alt: "Universal Inbox logo",
                        }
                    }

                    if let Some(error_message) = &UI_MODEL.read().error_message {
                        div { class: "alert alert-error text-sm", "{error_message}" }
                    }

                    if let Some(confirmation_message) = &UI_MODEL.read().confirmation_message {
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
                    } else {
                        Outlet::<Route> {}
                    }
                }
            }
        }
    }
}
