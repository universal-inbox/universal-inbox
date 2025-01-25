#![allow(non_snake_case)]

use dioxus::prelude::*;
use gravatar_rs::Generator;

use universal_inbox::FrontAuthenticationConfig;

use crate::{
    components::loading::Loading, config::APP_CONFIG, services::user_service::CONNECTED_USER,
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

    let user_profile_url = APP_CONFIG.read().as_ref().and_then(|config| {
        match config
            .authentication_configs
            .iter()
            .find(|auth_config| auth_config.match_user_auth(&user.auth))
        {
            Some(FrontAuthenticationConfig::OIDCAuthorizationCodePKCEFlow(config)) => {
                Some(config.user_profile_url.clone())
            }
            Some(FrontAuthenticationConfig::OIDCGoogleAuthorizationCodeFlow(config)) => {
                Some(config.user_profile_url.clone())
            }
            _ => None,
        }
    });

    let user_avatar = Generator::default()
        .set_image_size(150)
        .set_rating("g")
        .set_default_image("mp")
        .generate(user.email.as_str());
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
                            class: "w-24 rounded-full ring ring-primary ring-offset-base-100 ring-offset-2",
                            img { src: "{user_avatar}", alt: "{user_name}" }
                        }
                    }

                    div {
                        class: "flex flex-col gap-2 justify-center grow",

                        div {
                            class: "text-xl font-bold",
                            "{user_name}"
                        }

                        div {
                            class: "text-xl font-semibold",
                            "{user.email}"
                        }
                    }

                    if let Some(user_profile_url) = user_profile_url.as_ref() {
                        a {
                            class: "btn btn-primary",
                            href: "{user_profile_url}",
                            target: "_blank",
                            rel: "noopener noreferrer",
                            "View detailed profile"
                        }
                    }
                }
            }
        }
    }
}
