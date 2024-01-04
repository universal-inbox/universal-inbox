#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_router::prelude::*;
use email_address::EmailAddress;
use log::error;

use crate::{
    components::floating_label_inputs::FloatingLabelInputText, form::FormValues, route::Route,
    services::user_service::UserCommand,
};

pub fn PasswordResetPage(cx: Scope) -> Element {
    let user_service = use_coroutine_handle::<UserCommand>(cx).unwrap();
    let email = use_state(cx, || "".to_string());
    let force_validation = use_state(cx, || false);

    render! {
        div {
            class: "flex flex-col items-center justify-center pb-8",
            h1 {
                class: "text-lg font-bold",
                span { "Reset your password" }
            }
        }

        form {
            class: "flex flex-col justify-center gap-4 px-10 pb-8",
            onsubmit: |evt| {
                match FormValues(evt.values.clone()).try_into() {
                    Ok(email_address) => {
                        user_service.send(UserCommand::SendPasswordResetEmail(email_address));
                    },
                    Err(err) => {
                        force_validation.set(true);
                        error!("Failed to parse form values as EmailAddress: {err}");
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

            button {
                class: "btn btn-primary",
                r#type: "submit",
                "Get password reset link"
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
