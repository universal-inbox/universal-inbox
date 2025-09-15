#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{BsCheck, BsExclamationTriangle},
    Icon,
};
use gravatar_rs::Generator;

use crate::{
    components::loading::Loading,
    model::DEFAULT_USER_AVATAR,
    services::user_service::{UserCommand, CONNECTED_USER},
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
                        class: "avatar justify-center",

                        div {
                            class: "w-24 rounded-full ring-3 ring-primary ring-offset-base-100 ring-offset-2",
                            img { src: "{user_avatar}", alt: "{user_name}" }
                        }
                    }

                    div {
                        class: "flex flex-col gap-2 justify-center grow",

                        div {
                            class: "text-lg font-bold",
                            "{user_name}"
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
