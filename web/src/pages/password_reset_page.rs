#![allow(non_snake_case)]

use dioxus::prelude::*;
use email_address::EmailAddress;
use log::error;

use crate::{
    components::floating_label_inputs::FloatingLabelInputText, form::FormValues, route::Route,
    services::user_service::UserCommand,
};

pub fn PasswordResetPage() -> Element {
    let user_service = use_coroutine_handle::<UserCommand>();
    let email = use_signal(|| "".to_string());
    let mut force_validation = use_signal(|| false);

    rsx! {
        div {
            class: "flex flex-col items-center justify-center pb-8",
            h1 {
                class: "text-lg font-bold",
                span { "Reset your password" }
            }
        }

        form {
            class: "flex flex-col justify-center gap-4 px-10 pb-8",
            onsubmit: move |evt| {
                match FormValues(evt.values()).try_into() {
                    Ok(email_address) => {
                        user_service.send(UserCommand::SendPasswordResetEmail(email_address));
                    },
                    Err(err) => {
                        *force_validation.write() = true;
                        error!("Failed to parse form values as EmailAddress: {err}");
                    }
                }
            },

            FloatingLabelInputText::<EmailAddress> {
                name: "email".to_string(),
                label: Some("Email".to_string()),
                required: true,
                value: email,
                autofocus: true,
                force_validation: force_validation(),
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
                    class: "link-hover link link-primary label-text-alt",
                    to: Route::LoginPage {},
                    "Login to existing account"
                }
            }
        }
    }
}
