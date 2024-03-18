#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_router::prelude::*;
use log::error;

use universal_inbox::user::{Password, PasswordResetToken, UserId};

use crate::{
    components::floating_label_inputs::FloatingLabelInputText, form::FormValues, route::Route,
    services::user_service::UserCommand,
};

#[component]
pub fn PasswordUpdatePage(
    cx: Scope,
    user_id: UserId,
    password_reset_token: PasswordResetToken,
) -> Element {
    let user_service = use_coroutine_handle::<UserCommand>(cx).unwrap();
    let password = use_state(cx, || "".to_string());
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
                    Ok(password) => {
                        user_service.send(UserCommand::ResetPassword(*user_id, password_reset_token.clone(), password));
                    },
                    Err(err) => {
                        force_validation.set(true);
                        error!("Failed to parse form values as Password: {err}");
                    }
                }
            },

            FloatingLabelInputText::<Password> {
                name: "password".to_string(),
                label: Some("Password"),
                required: true,
                value: password.clone(),
                force_validation: *force_validation.current(),
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
                    class: "link-hover link label-text-alt",
                    to: Route::LoginPage {},
                    "Login to existing account"
                }
            }
        }
    }
}
