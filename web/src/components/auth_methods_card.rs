#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::bs_icons::{BsShieldLock, BsTrash},
};
use log::error;
use secrecy::SecretBox;

use universal_inbox::user::{
    Password, UserAuthKind, UserAuthMethod, UserAuthMethodDisplayInfo, Username,
};

use crate::{
    components::{floating_label_inputs::FloatingLabelInputText, loading::Loading},
    form::FormValues,
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
            div {
                class: "card w-full bg-base-200",
                Loading { label: "Loading authentication methods..." }
            }
        };
    };

    let method_count = auth_methods.len();
    let has_local = auth_methods.iter().any(|m| m.kind == UserAuthKind::Local);
    let has_passkey = auth_methods.iter().any(|m| m.kind == UserAuthKind::Passkey);

    rsx! {
        div {
            class: "card w-full bg-base-200",

            div {
                class: "card-body",
                div {
                    class: "flex flex-col gap-4",

                    div {
                        class: "card-title flex flex-row items-center",
                        figure { class: "p-2", Icon { class: "w-8 h-8", icon: BsShieldLock } }
                        "Authentication methods"
                    }

                    div {
                        class: "flex flex-col gap-2",
                        for method in auth_methods.iter() {
                            AuthMethodRow {
                                key: "{method.kind}",
                                method: method.clone(),
                                can_remove: method_count > 1,
                            }
                        }
                    }

                    div {
                        class: "flex flex-col gap-3",

                        if !has_local {
                            if show_add_password() {
                                form {
                                    class: "flex flex-col gap-2",
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
                                        class: "flex gap-2",
                                        button {
                                            class: "btn btn-primary btn-sm",
                                            r#type: "submit",
                                            "Add password"
                                        }
                                        button {
                                            class: "btn btn-ghost btn-sm",
                                            r#type: "button",
                                            onclick: move |_| {
                                                show_add_password.set(false);
                                                force_password_validation.set(false);
                                            },
                                            "Cancel"
                                        }
                                    }
                                }
                            } else {
                                button {
                                    class: "btn btn-outline btn-sm w-fit",
                                    onclick: move |_| show_add_password.set(true),
                                    "Add password"
                                }
                            }
                        }

                        if !has_passkey {
                            if show_add_passkey() {
                                form {
                                    class: "flex flex-col gap-2",
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
                                        class: "flex gap-2",
                                        button {
                                            class: "btn btn-primary btn-sm",
                                            r#type: "submit",
                                            "Add passkey"
                                        }
                                        button {
                                            class: "btn btn-ghost btn-sm",
                                            r#type: "button",
                                            onclick: move |_| {
                                                show_add_passkey.set(false);
                                                force_passkey_validation.set(false);
                                            },
                                            "Cancel"
                                        }
                                    }
                                }
                            } else {
                                button {
                                    class: "btn btn-outline btn-sm w-fit",
                                    onclick: move |_| show_add_passkey.set(true),
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

    let (label, badge_class) = match &method.display_info {
        UserAuthMethodDisplayInfo::Local => ("Password".to_string(), "badge-primary"),
        UserAuthMethodDisplayInfo::Passkey { username } => {
            (format!("Passkey: {username}"), "badge-secondary")
        }
        UserAuthMethodDisplayInfo::OIDCGoogleAuthorizationCode => {
            ("Google".to_string(), "badge-accent")
        }
        UserAuthMethodDisplayInfo::OIDCAuthorizationCodePKCE => ("OIDC".to_string(), "badge-info"),
    };

    rsx! {
        div {
            class: "flex items-center justify-between gap-2 p-2 rounded-lg bg-base-300",

            div {
                class: "flex items-center gap-2",
                span { class: "badge {badge_class} badge-sm", "{label}" }
            }

            if confirming_remove() {
                div {
                    class: "flex items-center gap-2",
                    span { class: "text-sm text-warning", "Remove?" }
                    button {
                        class: "btn btn-error btn-xs",
                        onclick: move |_| {
                            user_service.send(UserCommand::RemoveAuthMethod(kind));
                            confirming_remove.set(false);
                        },
                        "Confirm"
                    }
                    button {
                        class: "btn btn-ghost btn-xs",
                        onclick: move |_| confirming_remove.set(false),
                        "Cancel"
                    }
                }
            } else if can_remove {
                button {
                    class: "btn btn-ghost btn-sm text-error",
                    onclick: move |_| confirming_remove.set(true),
                    Icon { class: "w-4 h-4", icon: BsTrash }
                }
            }
        }
    }
}
