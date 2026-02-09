#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::bs_icons::{BsCheck, BsExclamationTriangle, BsPencilSquare},
};
use email_address::EmailAddress;
use gravatar_rs::Generator;

use universal_inbox::user::UserPatch;

use crate::{
    components::{floating_label_inputs::FloatingLabelInputText, loading::Loading},
    form::FormValues,
    model::DEFAULT_USER_AVATAR,
    services::user_service::{CONNECTED_USER, UserCommand},
};

#[component]
pub fn UserProfileCard() -> Element {
    let user_service = use_coroutine_handle::<UserCommand>();
    let Some(user) = CONNECTED_USER.read().clone() else {
        return rsx! {
            div {
                class: "card w-full bg-base-200",
                Loading { label: "Loading user profile..." }
            }
        };
    };

    let mut is_editing = use_signal(|| false);
    let mut force_validation = use_signal(|| false);
    let mut first_name = use_signal(String::new);
    let mut last_name = use_signal(String::new);
    let mut email = use_signal(String::new);

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
            class: "card w-full bg-base-200",

            div {
                class: "card-body",
                div {
                    class: "flex flex-col sm:flex-row gap-4",

                    div {
                        class: "avatar justify-center self-start",

                        div {
                            class: "w-24 shrink-0 rounded-full ring-3 ring-primary ring-offset-base-100 ring-offset-2",
                            img { src: "{user_avatar}", alt: "{user_name}" }
                        }
                    }

                    div {
                        class: "flex flex-col gap-2 justify-center grow",

                        if is_editing() {
                            form {
                                class: "flex flex-col gap-6",
                                onsubmit: move |evt| {
                                    force_validation.set(true);
                                    let form_values = FormValues(evt.values().into_iter().collect());
                                    if let Ok(patch) = UserPatch::try_from(form_values) {
                                        user_service.send(UserCommand::UpdateUser(patch));
                                        is_editing.set(false);
                                    }
                                },

                                FloatingLabelInputText::<String> {
                                    name: "first_name".to_string(),
                                    label: Some("First Name".to_string()),
                                    value: first_name,
                                    force_validation: force_validation(),
                                }

                                FloatingLabelInputText::<String> {
                                    name: "last_name".to_string(),
                                    label: Some("Last Name".to_string()),
                                    value: last_name,
                                    force_validation: force_validation(),
                                }

                                FloatingLabelInputText::<EmailAddress> {
                                    name: "email".to_string(),
                                    label: Some("Email".to_string()),
                                    value: email,
                                    force_validation: force_validation(),
                                }

                                div {
                                    class: "flex gap-2 mt-2",
                                    button {
                                        class: "btn btn-sm btn-primary",
                                        r#type: "submit",
                                        "Save"
                                    }
                                    button {
                                        class: "btn btn-sm btn-ghost",
                                        r#type: "button",
                                        onclick: move |_| {
                                            is_editing.set(false);
                                        },
                                        "Cancel"
                                    }
                                }
                            }
                        } else {
                            div {
                                class: "flex items-center gap-2",
                                div {
                                    class: "text-lg font-bold",
                                    "{user_name}"
                                }
                                button {
                                    class: "btn btn-sm btn-ghost btn-circle",
                                    onclick: {
                                        let user = user.clone();
                                        move |_| {
                                            first_name.set(user.first_name.clone().unwrap_or_default());
                                            last_name.set(user.last_name.clone().unwrap_or_default());
                                            email.set(user.email.as_ref().map(|e| e.to_string()).unwrap_or_default());
                                            is_editing.set(true);
                                        }
                                    },
                                    Icon { class: "min-w-4 h-4", icon: BsPencilSquare }
                                }
                            }

                            if let Some(ref email) = user.email {
                                div {
                                    class: "flex flex-col gap-1",
                                    div {
                                        class: "text-lg font-semibold",
                                        "{email}"
                                    }
                                    div {
                                        class: "flex items-center gap-2",
                                        if user.is_email_validated() {
                                            span {
                                                class: "badge badge-success badge-success gap-1",
                                                Icon { class: "min-w-5 h-5", icon: BsCheck }
                                                span { "Email verified" }
                                            }
                                        } else {
                                            span {
                                                class: "badge badge-warning badge-soft gap-1",
                                                Icon { class: "min-w-5 h-5", icon: BsExclamationTriangle }
                                                span { "Email not verified" }
                                            }
                                            button {
                                                class: "btn btn-sm btn-primary ml-2",
                                                onclick: move |_| {
                                                    user_service.send(UserCommand::ResendVerificationEmail);
                                                },
                                                "Resend Verification"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
