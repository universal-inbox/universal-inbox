#![allow(non_snake_case)]

use dioxus::prelude::*;
use email_address::EmailAddress;

use log::error;

use universal_inbox::user::Password;

use crate::{
    components::{
        floating_label_inputs::FloatingLabelInputText, universal_inbox_title::UniversalInboxTitle,
    },
    form::FormValues,
    route::Route,
    services::user_service::{UserCommand, CONNECTED_USER},
};

pub fn LoginPage() -> Element {
    let user_service = use_coroutine_handle::<UserCommand>();
    let email = use_signal(|| "".to_string());
    let password = use_signal(|| "".to_string());
    let mut force_validation = use_signal(|| false);
    let nav = use_navigator();

    if CONNECTED_USER.read().is_some() {
        nav.push(Route::NotificationsPage {});
        needs_update();
        return rsx! {};
    };

    rsx! {
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
            onsubmit: move |evt| {
                match FormValues(evt.values()).try_into() {
                    Ok(credentials) => {
                        user_service.send(UserCommand::Login(credentials));
                    },
                    Err(err) => {
                        *force_validation.write() = true;
                        error!("Failed to parse form values as Credentials: {err}");
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

            FloatingLabelInputText::<Password> {
                name: "password".to_string(),
                label: Some("Password".to_string()),
                required: true,
                value: password,
                force_validation: force_validation(),
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
