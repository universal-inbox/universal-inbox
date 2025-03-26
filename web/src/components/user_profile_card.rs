#![allow(non_snake_case)]

use dioxus::prelude::*;
use gravatar_rs::Generator;

use crate::{
    components::loading::Loading, model::DEFAULT_USER_AVATAR,
    services::user_service::CONNECTED_USER,
};

#[component]
pub fn UserProfileCard() -> Element {
    let Some(user) = CONNECTED_USER.read().clone() else {
        return rsx! {
            div {
                class: "card w-full bg-base-200 text-base-content",
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
        user.first_name.unwrap_or_default(),
        user.last_name.unwrap_or_default()
    );

    rsx! {
        div {
            class: "card w-full bg-base-200 text-base-content",

            div {
                class: "card-body",
                div {
                    class: "flex flex-row gap-4",

                    div {
                        class: "avatar",

                        div {
                            class: "w-24 rounded-full ring-3 ring-primary ring-offset-base-100 ring-offset-2",
                            img { src: "{user_avatar}", alt: "{user_name}" }
                        }
                    }

                    div {
                        class: "flex flex-col gap-2 justify-center grow",

                        div {
                            class: "text-xl font-bold",
                            "{user_name}"
                        }

                        if let Some(ref email) = user.email {
                            div {
                                class: "text-xl font-semibold",
                                "{email}"
                            }
                        }
                    }
                }
            }
        }
    }
}
