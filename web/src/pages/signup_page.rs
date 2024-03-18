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
    route::Route,
    services::user_service::{UserCommand, CONNECTED_USER},
};

pub fn SignupPage(cx: Scope) -> Element {
    let user_service = use_coroutine_handle::<UserCommand>(cx).unwrap();
    let first_name = use_state(cx, || "".to_string());
    let last_name = use_state(cx, || "".to_string());
    let email = use_state(cx, || "".to_string());
    let password = use_state(cx, || "".to_string());
    let force_validation = use_state(cx, || false);
    let connected_user_ref = use_atom_ref(cx, &CONNECTED_USER);
    let nav = use_navigator(cx);

    if connected_user_ref.read().is_some() {
        nav.push(Route::NotificationsPage {});
        cx.needs_update();
        return None;
    };

    render! {
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
                    label: Some("First name"),
                    required: true,
                    value: first_name.clone(),
                    autofocus: true,
                    force_validation: *force_validation.current(),
                }

                FloatingLabelInputText::<String> {
                    name: "last_name".to_string(),
                    label: Some("Last name"),
                    required: true,
                    value: last_name.clone(),
                    force_validation: *force_validation.current(),
                }
            }

            FloatingLabelInputText::<EmailAddress> {
                name: "email".to_string(),
                label: Some("Email"),
                required: true,
                value: email.clone(),
                force_validation: *force_validation.current(),
                r#type: "email".to_string()
            }

            FloatingLabelInputText::<Password> {
                name: "password".to_string(),
                label: Some("Password"),
                required: true,
                value: password.clone(),
                force_validation: *force_validation.current(),
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
