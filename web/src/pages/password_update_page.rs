#![allow(non_snake_case)]

use dioxus::prelude::*;
use log::error;

use universal_inbox::user::{Password, PasswordResetToken, UserId};

use crate::{
    components::floating_label_inputs::FloatingLabelInputText, form::FormValues, route::Route,
    services::user_service::UserCommand,
};

#[component]
pub fn PasswordUpdatePage(user_id: UserId, password_reset_token: PasswordResetToken) -> Element {
    let user_service = use_coroutine_handle::<UserCommand>();
    let password = use_signal(|| "".to_string());
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
                    Ok(password) => {
                        user_service.send(UserCommand::ResetPassword(user_id, password_reset_token.clone(), password));
                    },
                    Err(err) => {
                        *force_validation.write() = true;
                        error!("Failed to parse form values as Password: {err}");
                    }
                }
            },

            FloatingLabelInputText::<Password> {
                name: "password".to_string(),
                label: Some("Password".to_string()),
                required: true,
                value: password,
                force_validation: force_validation(),
                r#type: "password".to_string()
            }

            button {
                class: "btn btn-primary",
                r#type: "submit",
                "Reset password"
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
