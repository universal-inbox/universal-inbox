#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_router::prelude::*;
use email_address::EmailAddress;
use fermi::use_atom_ref;
use log::error;

use universal_inbox::user::Password;

use crate::{
    components::{
        floating_label_inputs::FloatingLabelInputText, universal_inbox_title::UniversalInboxTitle,
    },
    form::FormValues,
    model::UI_MODEL,
    route::Route,
    services::user_service::UserCommand,
};

pub fn SignupPage(cx: Scope) -> Element {
    let user_service = use_coroutine_handle::<UserCommand>(cx).unwrap();
    let ui_model_ref = use_atom_ref(cx, &UI_MODEL);
    let first_name = use_state(cx, || "".to_string());
    let last_name = use_state(cx, || "".to_string());
    let email = use_state(cx, || "".to_string());
    let password = use_state(cx, || "".to_string());
    let force_validation = use_state(cx, || false);

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
                            span { "Create a new " }
                            UniversalInboxTitle {}
                            span { " account" }
                        }
                    }

                    form {
                        class: "flex flex-col justify-center gap-4 px-10 pb-8",
                        onsubmit: |evt| {
                            match FormValues(evt.values.clone()).try_into() {
                                Ok(params) => {
                                    user_service.send(UserCommand::RegisterUser(params));
                                },
                                Err(err) => {
                                    force_validation.set(true);
                                    error!("Failed to parse form values as RegisterUserParameters: {err}");
                                }
                            }
                        },

                        div {
                            class: "flex flex-row justify-between gap-4",

                            FloatingLabelInputText::<String> {
                                name: "first_name".to_string(),
                                label: "First name".to_string(),
                                required: true,
                                value: first_name.clone(),
                                autofocus: true,
                                force_validation: *force_validation.current(),
                            }

                            FloatingLabelInputText::<String> {
                                name: "last_name".to_string(),
                                label: "Last name".to_string(),
                                required: true,
                                value: last_name.clone(),
                                force_validation: *force_validation.current(),
                            }
                        }

                        FloatingLabelInputText::<EmailAddress> {
                            name: "email".to_string(),
                            label: "Email".to_string(),
                            required: true,
                            value: email.clone(),
                            force_validation: *force_validation.current(),
                            r#type: "email".to_string()
                        }

                        FloatingLabelInputText::<Password> {
                            name: "password".to_string(),
                            label: "Password".to_string(),
                            required: true,
                            value: password.clone(),
                            force_validation: *force_validation.current(),
                            r#type: "password".to_string()
                        }

                        if let Some(error_message) = &ui_model_ref.read().error_message {
                            render! {
                                div { class: "alert alert-error text-sm", "{error_message}" }
                            }
                        }

                        button {
                            class: "btn btn-primary mt-2",
                            r#type: "submit",
                            "Signup"
                        }

                        div {
                            class: "label justify-end",
                            Link {
                                class: "link-hover link label-text-alt",
                                to: Route::LoginPage {},
                                "Login to existing account"
                            }
                        }
                    }
                }
            }
        }
    }
}
