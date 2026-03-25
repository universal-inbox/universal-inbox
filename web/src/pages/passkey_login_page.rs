#![allow(non_snake_case)]

use dioxus::prelude::dioxus_core::needs_update;
use dioxus::prelude::*;
use log::error;
use universal_inbox::FrontAuthenticationConfig;

use crate::{
    components::{
        auth_widgets::{Backlink, PrimaryBtn},
        floating_label_inputs::FloatingLabelInputText,
        loading::Loading,
        ui::PageHeader,
    },
    config::APP_CONFIG,
    form::FormValues,
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
        Backlink { to: Route::LoginPage {}, "Other ways to sign in" }
        PageHeader {
            title: "Sign in with a passkey".to_string(),
            subtitle: Some("No password needed, your device unlocks your inbox.".to_string()),
        }

        form {
            "novalidate": "true",
            onsubmit: move |evt| {
                evt.prevent_default();
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
                label: Some("Username or email".to_string()),
                required: true,
                value: username,
                autofocus: true,
                force_validation: force_validation(),
                field_icon_class: "icon-[lucide--user]".to_string(),
                placeholder: "username".to_string(),
            }

            PrimaryBtn {
                button_type: "submit".to_string(),
                icon_class: "icon-[lucide--fingerprint]".to_string(),
                "Continue with passkey"
            }
        }

        div { class: "mt-auto pt-6 text-center text-xs text-ui-base-muted",
            "New to Universal Inbox? "
            Link {
                class: "text-ui-primary font-semibold no-underline hover:text-ui-primary-hover hover:underline",
                to: Route::SignupPage {},
                "Create account"
            }
        }
    }
}
