#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsCheckCircle, Icon};
use dioxus_router::prelude::*;

use crate::route::Route;

pub fn RecoverPasswordPage(cx: Scope) -> Element {
    render! {
        div {
            class: "flex min-h-screen items-center justify-center bg-base-100",
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
                        h1 {
                            class: "text-lg font-bold",
                            span { "Recover your password" }
                        }
                    }

                    form {
                        class: "flex flex-col justify-center gap-4 px-10 pb-8",

                        div {
                            class: "alert alert-success text-xs",
                            Icon { class: "w-5 h-5", icon: BsCheckCircle }
                            span { "Recovery email sent successfully" }
                        }

                        div {
                            class: "form-control",
                            label {
                                class: "label",
                                "for": "email",
                                span { class: "label-text", "Email" }
                            }
                            input {
                                r#type: "email",
                                placeholder: "email",
                                class: "input input-bordered [&:user-invalid]:input-warning [&:user-valid]:input-success",
                                required: true,
                                id: "email"
                            }
                        }

                        button {
                            class: "btn btn-primary mt-2",
                            r#type: "submit",
                            "Recover"
                        }

                        div {
                            class: "label justify-end",
                            Link {
                                class: "link-hover link label-text-alt",
                                to: Route::LoginPage {},
                                "Login"
                            }
                        }
                    }
                }
            }
        }
    }
}
