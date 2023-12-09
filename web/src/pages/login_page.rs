#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_router::prelude::*;

use crate::{components::universal_inbox_title::UniversalInboxTitle, route::Route};

pub fn LoginPage(cx: Scope) -> Element {
    render! {
        body {
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
                            span { "Login to " }
                            UniversalInboxTitle {}
                        }
                    }

                    form {
                        class: "flex flex-col justify-center gap-4 px-10 pb-8",

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

                        div {
                            class: "form-control",
                            label {
                                class: "label",
                                "for": "password",
                                span { class: "label-text", "Password" }
                            }
                            input {
                                r#type: "password",
                                placeholder: "password",
                                class: "input input-bordered [&:user-invalid]:input-warning [&:user-valid]:input-success",
                                required: true,
                                minlength: "6",
                                "for": "password"
                            }
                        }

                        div {
                            class: "flex items-center justify-between gap-3",
                            label {
                                class: "flex cursor-pointer gap-3 text-xs",
                                input {
                                    name: "remember-me",
                                    r#type: "checkbox",
                                    class: "toggle toggle-xs"
                                }
                                "Remember me"
                            }
                            div {
                                class: "label",
                                Link {
                                    class: "link-hover link label-text-alt",
                                    to: Route::RecoverPasswordPage {},
                                    "Forgot password?"
                                }
                            }
                        }

                        button {
                            class: "btn btn-primary",
                            r#type: "submit",
                            "Login"
                        }

                        div {
                            class: "label justify-end",
                            Link {
                                class: "link-hover link label-text-alt",
                                to: Route::SignupPage {},
                                "Create new account"
                            }
                        }
                    }
                }
            }
        }
    }
}
