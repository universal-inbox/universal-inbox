#![allow(non_snake_case)]

use dioxus::prelude::dioxus_core::needs_update;
use dioxus::prelude::*;
use email_address::EmailAddress;
use log::error;

use universal_inbox::{FrontAuthenticationConfig, user::Password};

use crate::{
    auth::authenticate_authorization_code_flow,
    components::{
        auth_widgets::{AuthDivider, GoogleBtn, LegalFooter, PasskeyBtn, PrimaryBtn},
        floating_label_inputs::FloatingLabelInputText,
        loading::Loading,
        ui::PageHeader,
    },
    config::{APP_CONFIG, get_api_base_url},
    form::FormValues,
    route::Route,
    services::user_service::{CONNECTED_USER, UserCommand},
};

pub fn SignupPage() -> Element {
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
    let is_passkey_auth_enabled = app_config
        .authentication_configs
        .iter()
        .any(|auth_config| matches!(auth_config, FrontAuthenticationConfig::Passkey));

    rsx! {
        PageHeader {
            title: "Create your inbox".to_string(),
            subtitle: Some("Get triage-ready in under a minute.".to_string()),
        }
        p { class: "text-sm text-ui-base-muted leading-normal mb-7",
            "Already have an account? "
            Link {
                class: "text-ui-primary font-semibold no-underline hover:text-ui-primary-hover hover:underline",
                to: Route::LoginPage {},
                "Log in"
            }
            "."
        }

        if is_local_auth_enabled {
            form {
                "novalidate": "true",
                onsubmit: move |evt| {
                    evt.prevent_default();
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

                FloatingLabelInputText::<EmailAddress> {
                    name: "email".to_string(),
                    label: Some("Work email".to_string()),
                    required: true,
                    value: email,
                    force_validation: force_validation(),
                    r#type: "email".to_string(),
                    field_icon_class: "icon-[lucide--mail]".to_string(),
                    placeholder: "you@company.com".to_string(),
                }

                FloatingLabelInputText::<Password> {
                    name: "password".to_string(),
                    label: Some("Password".to_string()),
                    required: true,
                    value: password,
                    force_validation: force_validation(),
                    r#type: "password".to_string(),
                    field_icon_class: "icon-[lucide--lock]".to_string(),
                    placeholder: "At least 10 characters".to_string(),
                    help: "Use 10+ characters with a mix of letters, numbers and symbols.".to_string(),
                }

                div { class: "h-1.5" }

                PrimaryBtn { button_type: "submit".to_string(), "Create account" }
            }

            if is_google_auth_enabled || is_passkey_auth_enabled {
                AuthDivider { label: "or sign up with".to_string() }
            }

            div { class: "grid grid-cols-1 gap-2.5",
                if is_google_auth_enabled {
                    GoogleBtn {
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
                        "Sign up with Google"
                    }
                }
                if is_passkey_auth_enabled {
                    PasskeyBtn { to: Route::PasskeySignupPage {}, "Sign up with a passkey" }
                }
            }

            LegalFooter {}
        } else {
            div { class: "mt-auto pt-6 text-center text-xs text-ui-base-muted",
                "Already have an account? "
                Link {
                    class: "text-ui-primary font-semibold no-underline hover:text-ui-primary-hover hover:underline",
                    to: Route::LoginPage {},
                    "Log in"
                }
            }
        }
    }
}
