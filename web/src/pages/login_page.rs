#![allow(non_snake_case)]

use dioxus::prelude::*;
use email_address::EmailAddress;
use log::error;

use universal_inbox::{user::Password, FrontAuthenticationConfig};

use crate::{
    auth::authenticate_authorization_code_flow,
    components::{
        floating_label_inputs::FloatingLabelInputText, loading::Loading,
        universal_inbox_title::UniversalInboxTitle,
    },
    config::{get_api_base_url, APP_CONFIG},
    form::FormValues,
    icons::GOOGLE_LOGO,
    route::Route,
    services::user_service::{UserCommand, CONNECTED_USER},
};

pub fn LoginPage() -> Element {
    let api_base_url = use_memo(move || get_api_base_url().unwrap());
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

    let app_config = APP_CONFIG.read();
    let Some(app_config) = app_config.as_ref() else {
        return rsx! { Loading { label: "Loading Universal Inbox settings..." } };
    };
    let is_local_auth_enabled = app_config
        .authentication_configs
        .iter()
        .any(|auth_config| matches!(auth_config, FrontAuthenticationConfig::Local));
    let is_google_auth_enabled = app_config.authentication_configs.iter().any(|auth_config| {
        matches!(
            auth_config,
            FrontAuthenticationConfig::OIDCGoogleAuthorizationCodeFlow(_)
        )
    });
    let form_style = if is_google_auth_enabled { "" } else { "pb-8" };

    rsx! {
        div {
            class: "flex flex-col items-center justify-center pb-8",
            h1 {
                class: "text-lg font-bold",
                span { "Login to " }
                UniversalInboxTitle {}
            }
        }

        if is_local_auth_enabled {
            form {
                class: "flex flex-col justify-center gap-4 px-10 {form_style}",
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
                            class: "link-hover link label-text-alt link-primary",
                            to: Route::PasswordResetPage {},
                            "Forgot password?"
                        }
                    }
                }

                button {
                    class: "btn btn-primary",
                    r#type: "submit",
                    "Log in"
                }
            }

            if is_google_auth_enabled {
                div {
                    class: "flex flex-col px-10 pb-8",

                    div { class: "divider", "Or" }

                    button {
                        class: "btn btn-primary w-full relative",
                        onclick: move |_| {
                            spawn({
                                async move {
                                    if let Err(auth_error) =
                                        authenticate_authorization_code_flow(&api_base_url()).await
                                    {
                                        error!("An error occured while authenticating: {:?}", auth_error);
                                    }
                                }
                            });
                        },

                        img {
                            class: "h-8 w-8 bg-white rounded-md absolute left-2",
                            src: "{GOOGLE_LOGO}",
                        }
                        "Sign in with Google"
                    }
                }
            }

            div {
                class: "text-base px-10",
                span { "New to " }
                UniversalInboxTitle {}
                span { "? " }
                Link {
                    class: "link-hover link link-primary font-bold",
                    to: Route::SignupPage {},
                    "Create new account"
                }
            }
        }
    }
}
