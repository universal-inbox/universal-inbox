#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_router::prelude::*;
use email_address::EmailAddress;
use log::error;

use universal_inbox::user::Password;

use crate::{
    components::{
        floating_label_inputs::FloatingLabelInputText, universal_inbox_title::UniversalInboxTitle,
    },
    form::FormValues,
    route::Route,
    services::user_service::UserCommand,
};

pub fn LoginPage(cx: Scope) -> Element {
    let user_service = use_coroutine_handle::<UserCommand>(cx).unwrap();
    let email = use_state(cx, || "".to_string());
    let password = use_state(cx, || "".to_string());
    let force_validation = use_state(cx, || false);

    render! {
        div {
            class: "flex flex-col items-center justify-center pb-8",
            h1 {
                class: "text-lg font-bold",
                span { "Login to " }
                UniversalInboxTitle {}
            }
        }

        form {
            class: "flex flex-col justify-center gap-4 px-10 pb-8",
            onsubmit: |evt| {
                match FormValues(evt.values.clone()).try_into() {
                    Ok(credentials) => {
                        user_service.send(UserCommand::Login(credentials));
                    },
                    Err(err) => {
                        force_validation.set(true);
                        error!("Failed to parse form values as Credentials: {err}");
                    }
                }
            },

            FloatingLabelInputText::<EmailAddress> {
                name: "email".to_string(),
                label: "Email".to_string(),
                required: true,
                value: email.clone(),
                autofocus: true,
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

            div {
                class: "flex items-center justify-end",
                div {
                    class: "label",
                    Link {
                        class: "link-hover link label-text-alt",
                        to: Route::PasswordResetPage {},
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
