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

pub fn SignupPage() -> Element {
    let user_service = use_coroutine_handle::<UserCommand>();
    let first_name = use_signal(|| "".to_string());
    let last_name = use_signal(|| "".to_string());
    let email = use_signal(|| "".to_string());
    let password = use_signal(|| "".to_string());
    let mut force_validation = use_signal(|| false);
    let nav = use_navigator();

    if CONNECTED_USER.read().is_some() {
        nav.push(Route::NotificationsPage {});
        needs_update();
        return None;
    };

    rsx! {
        div {
            class: "flex flex-col items-center justify-center pb-8",
            h1 {
                class: "text-lg font-bold",
                span { "Create a new " }
                UniversalInboxTitle {}
                span { " account" }
            }
        }

        form {
            class: "flex flex-col justify-center gap-4 px-10 pb-8",
            onsubmit: move |evt| {
                match FormValues(evt.values()).try_into() {
                    Ok(params) => {
                        user_service.send(UserCommand::RegisterUser(params));
                    },
                    Err(err) => {
                        *force_validation.write() = true;
                        error!("Failed to parse form values as RegisterUserParameters: {err}");
                    }
                }
            },

            div {
                class: "flex flex-row justify-between gap-4",

                FloatingLabelInputText::<String> {
                    name: "first_name".to_string(),
                    label: Some("First name".to_string()),
                    required: true,
                    value: first_name,
                    autofocus: true,
                    force_validation: force_validation(),
                }

                FloatingLabelInputText::<String> {
                    name: "last_name".to_string(),
                    label: Some("Last name".to_string()),
                    required: true,
                    value: last_name,
                    force_validation: force_validation(),
                }
            }

            FloatingLabelInputText::<EmailAddress> {
                name: "email".to_string(),
                label: Some("Email".to_string()),
                required: true,
                value: email,
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
