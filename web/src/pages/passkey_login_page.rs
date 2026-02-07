#![allow(non_snake_case)]

use dioxus::prelude::*;
use log::error;
use universal_inbox::FrontAuthenticationConfig;

use crate::{
    components::{
        floating_label_inputs::FloatingLabelInputText, loading::Loading,
        universal_inbox_title::UniversalInboxTitle,
    },
    config::APP_CONFIG,
    form::FormValues,
    icons::PASSKEY_LOGO,
    route::Route,
    services::user_service::{CONNECTED_USER, UserCommand},
};

pub fn PasskeyLoginPage() -> Element {
    let user_service = use_coroutine_handle::<UserCommand>();
    let username = use_signal(|| "".to_string());
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
    let is_passkey_auth_enabled = app_config
        .authentication_configs
        .iter()
        .any(|auth_config| matches!(auth_config, FrontAuthenticationConfig::Passkey));

    if !is_passkey_auth_enabled {
        return rsx! {};
    }

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
                        Ok(username) => {
                            user_service.send(UserCommand::LoginPasskey(username));
                        },
                        Err(err) => {
                            *force_validation.write() = true;
                            error!("Failed to parse form values as Username: {err}");
                        }
                    }
                },

            FloatingLabelInputText::<String> {
                name: "username".to_string(),
                label: Some("Username".to_string()),
                required: true,
                value: username,
                autofocus: true,
                force_validation: force_validation(),
            }

            button {
                class: "btn btn-primary relative",
                r#type: "submit",

                img {
                    class: "h-8 w-8 bg-white rounded-md absolute left-2",
                    src: "{PASSKEY_LOGO}",
                }
                "Log in with a passkey"
            }
        }

        div {
            class: "text-base px-10",
            span { "New to " }
            UniversalInboxTitle {}
            span { "? " }
            Link {
                class: "link-hover link link-primary",
                to: Route::SignupPage {},
                "Create new account"
            }
        }
    }
}
