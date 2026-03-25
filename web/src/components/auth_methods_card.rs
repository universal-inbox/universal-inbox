#![allow(non_snake_case)]

use dioxus::prelude::*;
use log::error;
use secrecy::SecretBox;

use universal_inbox::{
    FrontAuthenticationConfig,
    user::{Password, UserAuthKind, UserAuthMethod, UserAuthMethodDisplayInfo, Username},
};

use crate::{
    components::{
        floating_label_inputs::FloatingLabelInputText,
        loading::Loading,
        ui::{
            Badge, BadgeTone, BadgeVariant, Button, ButtonVariant, Card, CardHeader, CardVariant,
        },
    },
    config::APP_CONFIG,
    form::FormValues,
    icons::GOOGLE_LOGO,
    services::user_service::{AUTH_METHODS, UserCommand},
};

#[component]
pub fn AuthMethodsCard() -> Element {
    let user_service = use_coroutine_handle::<UserCommand>();
    let mut show_add_password = use_signal(|| false);
    let mut show_add_passkey = use_signal(|| false);
    let password = use_signal(|| "".to_string());
    let passkey_username = use_signal(|| "".to_string());
    let mut force_password_validation = use_signal(|| false);
    let mut force_passkey_validation = use_signal(|| false);

    let _resource = use_resource(move || {
        to_owned![user_service];

        async move {
            user_service.send(UserCommand::ListAuthMethods);
        }
    });

    let Some(auth_methods) = AUTH_METHODS.read().clone() else {
        return rsx! {
            Card { variant: CardVariant::ApiKeys,
                Loading { label: "Loading authentication methods..." }
            }
        };
    };

    let method_count = auth_methods.len();
    let has_local = auth_methods.iter().any(|m| m.kind == UserAuthKind::Local);
    let has_passkey = auth_methods.iter().any(|m| m.kind == UserAuthKind::Passkey);
    let has_google = auth_methods
        .iter()
        .any(|m| m.kind == UserAuthKind::OIDCGoogleAuthorizationCode);
    let is_google_auth_enabled = APP_CONFIG
        .read()
        .as_ref()
        .map(|config| {
            config.authentication_configs.iter().any(|auth_config| {
                matches!(
                    auth_config,
                    FrontAuthenticationConfig::OIDCGoogleAuthorizationCodeFlow(_)
                )
            })
        })
        .unwrap_or(false);

    rsx! {
        section {
            role: "region",
            aria_label: "Authentication methods",

            Card { variant: CardVariant::ApiKeys,
                CardHeader {
                    span { class: "icon-[lucide--shield-check] size-5" }
                    "Authentication methods"
                }

                div {
                    class: "flex flex-col gap-1.5 px-4 py-2",

                    for method in auth_methods.iter() {
                        AuthMethodRow {
                            key: "{method.kind}",
                            method: method.clone(),
                            can_remove: method_count > 1,
                        }
                    }
                }

                div {
                    class: "flex flex-wrap gap-2 px-4 pt-2 pb-4",

                    if !has_local {
                        if show_add_password() {
                            form {
                                class: "flex flex-col gap-4",
                                onsubmit: move |evt| {
                                    evt.prevent_default();
                                    let result: Result<SecretBox<Password>, _> =
                                        FormValues(evt.values()).try_into();
                                    match result {
                                        Ok(new_password) => {
                                            user_service.send(
                                                UserCommand::AddLocalAuth(new_password),
                                            );
                                            show_add_password.set(false);
                                            force_password_validation.set(false);
                                        }
                                        Err(err) => {
                                            *force_password_validation.write() = true;
                                            error!(
                                                "Failed to parse form values as Password: {err}"
                                            );
                                        }
                                    }
                                },

                                FloatingLabelInputText::<Password> {
                                    name: "password".to_string(),
                                    label: Some("Password".to_string()),
                                    required: true,
                                    value: password,
                                    autofocus: true,
                                    force_validation: force_password_validation(),
                                    r#type: "password".to_string(),
                                }

                                div {
                                    class: "flex gap-2 mt-1",
                                    Button {
                                        variant: ButtonVariant::Ghost,
                                        onclick: move |_| {
                                            show_add_password.set(false);
                                            force_password_validation.set(false);
                                        },
                                        "Cancel"
                                    }
                                    Button {
                                        variant: ButtonVariant::Primary,
                                        button_type: "submit".to_string(),
                                        "Add password"
                                    }
                                }
                            }
                        } else {
                            button {
                                class: "group flex flex-1 min-w-[120px] flex-col items-center gap-2 \
                                        px-3 py-3 bg-ui-surface border border-ui-border rounded-ui-md \
                                        hover:border-ui-primary hover:bg-ui-surface-hover \
                                        focus-visible:outline-2 focus-visible:outline-ui-primary focus-visible:outline-offset-2 \
                                        transition-colors cursor-pointer",
                                r#type: "button",
                                onclick: move |_| show_add_password.set(true),
                                div {
                                    class: "flex items-center justify-center size-7 rounded-ui-sm bg-ui-surface-alt",
                                    span {
                                        class: "icon-[lucide--key-round] size-4 \
                                                text-ui-base-muted group-hover:text-ui-primary transition-colors",
                                    }
                                }
                                span {
                                    class: "text-[12.5px] font-medium text-ui-base-content",
                                    "Add password"
                                }
                            }
                        }
                    }

                    if is_google_auth_enabled && !has_google {
                        button {
                            class: "group flex flex-1 min-w-[120px] flex-col items-center gap-2 \
                                    px-3 py-3 bg-ui-surface border border-ui-border rounded-ui-md \
                                    hover:border-ui-primary hover:bg-ui-surface-hover \
                                    focus-visible:outline-2 focus-visible:outline-ui-primary focus-visible:outline-offset-2 \
                                    transition-colors cursor-pointer",
                            r#type: "button",
                            onclick: move |_| {
                                user_service.send(UserCommand::LinkOIDCAuth);
                            },
                            div {
                                class: "flex items-center justify-center size-7 rounded-ui-sm bg-ui-surface-alt",
                                img {
                                    class: "size-4",
                                    src: "{GOOGLE_LOGO}",
                                    alt: "Google logo",
                                }
                            }
                            span {
                                class: "text-[12.5px] font-medium text-ui-base-content",
                                "Link Google account"
                            }
                        }
                    }

                    if !has_passkey {
                        if show_add_passkey() {
                            form {
                                class: "flex flex-col gap-4",
                                onsubmit: move |evt| {
                                    evt.prevent_default();
                                    let result: Result<Username, _> =
                                        FormValues(evt.values()).try_into();
                                    match result {
                                        Ok(username) => {
                                            user_service.send(
                                                UserCommand::AddPasskeyAuthMethod(username),
                                            );
                                            show_add_passkey.set(false);
                                            force_passkey_validation.set(false);
                                        }
                                        Err(err) => {
                                            *force_passkey_validation.write() = true;
                                            error!(
                                                "Failed to parse form values as Username: {err}"
                                            );
                                        }
                                    }
                                },

                                FloatingLabelInputText::<String> {
                                    name: "username".to_string(),
                                    label: Some("Passkey username".to_string()),
                                    required: true,
                                    value: passkey_username,
                                    autofocus: true,
                                    force_validation: force_passkey_validation(),
                                    r#type: "text".to_string(),
                                }

                                div {
                                    class: "flex gap-2 mt-1",
                                    Button {
                                        variant: ButtonVariant::Ghost,
                                        onclick: move |_| {
                                            show_add_passkey.set(false);
                                            force_passkey_validation.set(false);
                                        },
                                        "Cancel"
                                    }
                                    Button {
                                        variant: ButtonVariant::Primary,
                                        button_type: "submit".to_string(),
                                        "Add passkey"
                                    }
                                }
                            }
                        } else {
                            button {
                                class: "group flex flex-1 min-w-[120px] flex-col items-center gap-2 \
                                        px-3 py-3 bg-ui-surface border border-ui-border rounded-ui-md \
                                        hover:border-ui-primary hover:bg-ui-surface-hover \
                                        focus-visible:outline-2 focus-visible:outline-ui-primary focus-visible:outline-offset-2 \
                                        transition-colors cursor-pointer",
                                r#type: "button",
                                onclick: move |_| show_add_passkey.set(true),
                                div {
                                    class: "flex items-center justify-center size-7 rounded-ui-sm bg-ui-surface-alt",
                                    span {
                                        class: "icon-[lucide--fingerprint] size-4 \
                                                text-ui-base-muted group-hover:text-ui-primary transition-colors",
                                    }
                                }
                                span {
                                    class: "text-[12.5px] font-medium text-ui-base-content",
                                    "Add passkey"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AuthMethodRow(method: UserAuthMethod, can_remove: bool) -> Element {
    let user_service = use_coroutine_handle::<UserCommand>();
    let mut confirming_remove = use_signal(|| false);
    let kind = method.kind;

    let (label, tone, icon): (String, BadgeTone, Element) = match &method.display_info {
        UserAuthMethodDisplayInfo::Local => (
            "Password".to_string(),
            BadgeTone::Primary,
            rsx! { span { class: "icon-[lucide--key-round] size-3" } },
        ),
        UserAuthMethodDisplayInfo::Passkey { username } => (
            format!("Passkey: {username}"),
            BadgeTone::Purple,
            rsx! { span { class: "icon-[lucide--fingerprint] size-3" } },
        ),
        UserAuthMethodDisplayInfo::OIDCGoogleAuthorizationCode => (
            "Google".to_string(),
            BadgeTone::Error,
            rsx! {
                img {
                    class: "size-3 rounded-[2px] bg-white",
                    src: "{GOOGLE_LOGO}",
                    alt: "Google",
                }
            },
        ),
        UserAuthMethodDisplayInfo::OIDCAuthorizationCodePKCE => (
            "OIDC".to_string(),
            BadgeTone::Info,
            rsx! { span { class: "icon-[lucide--shield-check] size-3" } },
        ),
    };

    rsx! {
        div {
            class: "flex items-center justify-between gap-2 px-3 py-2 rounded-ui-md bg-ui-base-200 border border-ui-border-light",

            div {
                class: "flex items-center gap-2",
                Badge { variant: BadgeVariant::Method, tone,
                    {icon}
                    "{label}"
                }
            }

            if confirming_remove() {
                div {
                    class: "flex items-center justify-end gap-1",
                    Button {
                        variant: ButtonVariant::Danger,
                        onclick: move |_| {
                            user_service.send(UserCommand::RemoveAuthMethod(kind));
                            confirming_remove.set(false);
                        },
                        "Confirm"
                    }
                    Button {
                        variant: ButtonVariant::Ghost,
                        onclick: move |_| confirming_remove.set(false),
                        "Cancel"
                    }
                }
            } else if can_remove {
                Button {
                    variant: ButtonVariant::Danger,
                    icon_class: "icon-[lucide--trash-2]".to_string(),
                    onclick: move |_| confirming_remove.set(true),
                    ""
                }
            }
        }
    }
}
