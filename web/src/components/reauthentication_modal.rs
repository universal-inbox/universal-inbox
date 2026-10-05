#![allow(non_snake_case)]
//! Re-authentication modal driven by `REAUTHENTICATION_TRIGGER`. Mounted once
//! at app level so any sensitive account operation the API refuses for lack
//! of a recent login can ask the user to confirm their identity, with any of
//! the login methods of their account.

use dioxus::prelude::*;
use log::error;

use universal_inbox::user::{Password, UserAuthKind};

use crate::{
    components::{
        floating_label_inputs::FloatingLabelInputText,
        ui::button::{Button, ButtonVariant},
    },
    form::{FormValues, ReauthenticationPassword},
    icons::GOOGLE_LOGO,
    services::user_service::{AUTH_METHODS, REAUTHENTICATION_TRIGGER, UserCommand},
};

#[component]
pub fn ReauthenticationModal() -> Element {
    if !REAUTHENTICATION_TRIGGER() {
        return rsx! { Fragment {} };
    }

    rsx! { ReauthenticationDialog {} }
}

#[component]
fn ReauthenticationDialog() -> Element {
    let user_service = use_coroutine_handle::<UserCommand>();
    let password = use_signal(|| "".to_string());
    let mut force_validation = use_signal(|| false);

    // The settings page usually loaded them already.
    use_effect(move || {
        if AUTH_METHODS.read().is_none() {
            user_service.send(UserCommand::ListAuthMethods);
        }
    });
    let auth_kinds: Vec<UserAuthKind> = AUTH_METHODS
        .read()
        .as_ref()
        .map(|methods| methods.iter().map(|method| method.kind).collect())
        .unwrap_or_default();
    let has_password = auth_kinds.contains(&UserAuthKind::Local);
    let has_passkey = auth_kinds.contains(&UserAuthKind::Passkey);
    let has_google = auth_kinds.contains(&UserAuthKind::OIDCGoogleAuthorizationCode);
    let has_single_sign_on_only =
        !auth_kinds.is_empty() && !has_password && !has_passkey && !has_google;

    let dismiss = move |_| {
        *REAUTHENTICATION_TRIGGER.write() = false;
    };

    rsx! {
        div {
            class: "fixed inset-0 z-[2000] flex items-center justify-center bg-black/45 px-4",
            role: "dialog",
            "aria-modal": "true",
            "aria-labelledby": "reauthentication-modal-headline",
            onclick: dismiss,
            div {
                class: "max-w-[420px] w-full rounded-ui-md border border-ui-border bg-ui-base-100 \
                        shadow-ui-lg p-5 font-ui flex flex-col gap-4",
                onclick: move |e| e.stop_propagation(),

                div {
                    h2 {
                        id: "reauthentication-modal-headline",
                        class: "text-sm font-semibold text-ui-base-content mb-2 flex items-center gap-2",
                        span { class: "icon-[lucide--shield-check] size-4 text-ui-primary" }
                        "Confirm it's you"
                    }
                    p {
                        class: "text-xs text-ui-base-muted leading-normal",
                        "This change affects how you sign in. Please confirm your identity, then retry it."
                    }
                }

                if has_password {
                    form {
                        class: "flex flex-col gap-3",
                        "novalidate": "true",
                        onsubmit: move |evt| {
                            evt.prevent_default();
                            match ReauthenticationPassword::try_from(FormValues(evt.values())) {
                                Ok(ReauthenticationPassword(password)) => {
                                    user_service.send(UserCommand::ReauthenticateWithPassword(password));
                                }
                                Err(err) => {
                                    force_validation.set(true);
                                    error!("Failed to parse form values as Password: {err}");
                                }
                            }
                        },

                        FloatingLabelInputText::<Password> {
                            name: "password".to_string(),
                            label: Some("Password".to_string()),
                            required: true,
                            value: password,
                            autofocus: true,
                            force_validation: force_validation(),
                            r#type: "password".to_string(),
                        }

                        Button {
                            variant: ButtonVariant::Primary,
                            button_type: "submit".to_string(),
                            "Confirm with my password"
                        }
                    }
                }

                if has_passkey {
                    Button {
                        variant: if has_password { ButtonVariant::Ghost } else { ButtonVariant::Primary },
                        onclick: move |_| user_service.send(UserCommand::ReauthenticateWithPasskey),
                        span { class: "icon-[lucide--key-round] size-4" }
                        "Confirm with my passkey"
                    }
                }

                if has_google {
                    Button {
                        variant: ButtonVariant::Ghost,
                        onclick: move |_| user_service.send(UserCommand::ReauthenticateWithGoogle),
                        img { class: "size-4", src: "{GOOGLE_LOGO}", alt: "Google" }
                        "Confirm with Google"
                    }
                }

                if has_single_sign_on_only {
                    p {
                        class: "text-xs text-ui-base-muted leading-normal",
                        "Log out and log back in, then retry."
                    }
                }

                div {
                    class: "flex justify-end",
                    Button {
                        variant: ButtonVariant::Ghost,
                        onclick: dismiss,
                        "Cancel"
                    }
                }
            }
        }
    }
}
