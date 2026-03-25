#![allow(non_snake_case)]

use dioxus::prelude::*;
use gravatar_rs::Generator;

use universal_inbox::user::UserPatch;

use universal_inbox::user::UserAuthKind;

use crate::{
    components::{
        loading::Loading,
        ui::{Badge, BadgeTone, BadgeVariant, Button, ButtonVariant},
    },
    form::FormValues,
    model::DEFAULT_USER_AVATAR,
    services::user_service::{AUTH_METHODS, CONNECTED_USER, UserCommand},
};

#[component]
pub fn UserProfileCard() -> Element {
    let user_service = use_coroutine_handle::<UserCommand>();

    let Some(user) = CONNECTED_USER.read().clone() else {
        return rsx! {
            div {
                class: "flex items-center gap-5 p-5 bg-ui-surface border border-ui-border rounded-ui-lg shadow-ui-sm",
                Loading { label: "Loading user profile..." }
            }
        };
    };

    let mut is_editing = use_signal(|| false);
    let mut first_name = use_signal(String::new);
    let mut last_name = use_signal(String::new);
    let mut email = use_signal(String::new);

    let _has_oidc = AUTH_METHODS
        .read()
        .as_ref()
        .map(|methods| {
            methods.iter().any(|m| {
                matches!(
                    m.kind,
                    UserAuthKind::OIDCGoogleAuthorizationCode
                        | UserAuthKind::OIDCAuthorizationCodePKCE
                )
            })
        })
        .unwrap_or(false);

    let user_avatar = if let Some(ref email) = user.email {
        Generator::default()
            .set_image_size(150)
            .set_rating("g")
            .set_default_image("mp")
            .generate(email.as_str())
    } else {
        DEFAULT_USER_AVATAR.to_string()
    };
    let user_name = format!(
        "{} {}",
        user.first_name.as_ref().unwrap_or(&String::default()),
        user.last_name.as_ref().unwrap_or(&String::default())
    );

    rsx! {
        div {
            class: "flex items-center gap-5 p-5 bg-ui-surface border border-ui-border rounded-ui-lg shadow-ui-sm",
            role: "region",
            aria_label: "User profile",

            div {
                class: "relative shrink-0",
                img { class: "profile-avatar max-md:w-16 max-md:h-16", src: "{user_avatar}", alt: "{user_name}" }
            }

            div {
                class: "flex-1 min-w-0",

                if is_editing() {
                    form {
                        class: "flex flex-col gap-4",
                        onsubmit: move |evt| {
                            let form_values = FormValues(evt.values().into_iter().collect());
                            if let Ok(patch) = UserPatch::try_from(form_values) {
                                user_service.send(UserCommand::UpdateUser(patch));
                                is_editing.set(false);
                            }
                        },

                        div {
                            class: "flex gap-3",
                            div {
                                class: "flex-1",
                                label { class: "block text-xs font-semibold uppercase tracking-wider text-ui-base-muted mb-1", r#for: "profFirstName", "First name" }
                                input {
                                    class: "w-full px-2.5 py-1.5 text-[var(--ui-text-base)] font-ui bg-ui-base-200 text-ui-base-content border border-ui-border rounded-ui-sm outline-none transition-[border-color,box-shadow] duration-150 ease-[var(--ui-ease)] focus:border-ui-primary focus:shadow-[var(--ui-focus-ring)] focus:outline-none",
                                    id: "profFirstName",
                                    name: "first_name",
                                    r#type: "text",
                                    value: "{first_name}",
                                    oninput: move |evt| first_name.set(evt.value()),
                                }
                            }
                            div {
                                class: "flex-1",
                                label { class: "block text-xs font-semibold uppercase tracking-wider text-ui-base-muted mb-1", r#for: "profLastName", "Last name" }
                                input {
                                    class: "w-full px-2.5 py-1.5 text-[var(--ui-text-base)] font-ui bg-ui-base-200 text-ui-base-content border border-ui-border rounded-ui-sm outline-none transition-[border-color,box-shadow] duration-150 ease-[var(--ui-ease)] focus:border-ui-primary focus:shadow-[var(--ui-focus-ring)] focus:outline-none",
                                    id: "profLastName",
                                    name: "last_name",
                                    r#type: "text",
                                    value: "{last_name}",
                                    oninput: move |evt| last_name.set(evt.value()),
                                }
                            }
                        }

                        div {
                            class: "flex-1",
                            label { class: "block text-xs font-semibold uppercase tracking-wider text-ui-base-muted mb-1", r#for: "profEmail", "Email" }
                            input {
                                class: "w-full px-2.5 py-1.5 text-[var(--ui-text-base)] font-ui bg-ui-base-200 text-ui-base-content border border-ui-border rounded-ui-sm outline-none transition-[border-color,box-shadow] duration-150 ease-[var(--ui-ease)] focus:border-ui-primary focus:shadow-[var(--ui-focus-ring)] focus:outline-none",
                                id: "profEmail",
                                name: "email",
                                r#type: "email",
                                value: "{email}",
                                oninput: move |evt| email.set(evt.value()),
                            }
                        }

                        div {
                            class: "flex gap-2 mt-1",
                            Button {
                                variant: ButtonVariant::Primary,
                                button_type: "submit".to_string(),
                                "Save changes"
                            }
                            Button {
                                variant: ButtonVariant::Ghost,
                                onclick: move |_| {
                                    is_editing.set(false);
                                },
                                "Cancel"
                            }
                        }
                    }
                } else {
                    div {
                        class: "profile-name text-lg font-bold text-ui-base-content tracking-tight flex gap-2 max-md:justify-center",
                        "{user_name}"
                        button {
                            class: "edit-btn max-md:w-9 max-md:h-9",
                            aria_label: "Edit profile",
                            onclick: {
                                let user = user.clone();
                                move |_| {
                                    first_name.set(user.first_name.clone().unwrap_or_default());
                                    last_name.set(user.last_name.clone().unwrap_or_default());
                                    email.set(user.email.as_ref().map(|e| e.to_string()).unwrap_or_default());
                                    is_editing.set(true);
                                }
                            },
                            span { class: "icon-[lucide--square-pen] size-3" }
                        }
                    }

                    if let Some(ref email) = user.email {
                        div {
                            class: "text-sm text-ui-base-muted mt-0.5 gap-2 flex",
                            "{email}"
                            if user.is_email_validated() {
                                Badge {
                                    variant: BadgeVariant::Email,
                                    tone: BadgeTone::Success,
                                    span { class: "icon-[lucide--check] size-3" }
                                    "Verified"
                                }
                            } else {
                                Badge {
                                    variant: BadgeVariant::Email,
                                    tone: BadgeTone::Warning,
                                    span { class: "icon-[lucide--triangle-alert] size-3" }
                                    "Not verified"
                                }
                            }
                        }
                        if !user.is_email_validated() {
                            div {
                                class: "mt-1.5",
                                Button {
                                    variant: ButtonVariant::Primary,
                                    onclick: move |_| {
                                        user_service.send(UserCommand::ResendVerificationEmail);
                                    },
                                    "Resend verification"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
