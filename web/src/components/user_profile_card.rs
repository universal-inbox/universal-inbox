#![allow(non_snake_case)]

use dioxus::prelude::*;
use fermi::use_atom_ref;
use gravatar::{Gravatar, Rating};

use universal_inbox::FrontAuthenticationConfig;

use crate::{
    components::spinner::Spinner, config::APP_CONFIG, services::user_service::CONNECTED_USER,
};

#[component]
pub fn UserProfileCard(cx: Scope) -> Element {
    let app_config_ref = use_atom_ref(cx, &APP_CONFIG);
    // Howto use use_memo with an Option?
    let user_profile_url =
        app_config_ref
            .read()
            .as_ref()
            .and_then(|config| match &config.authentication_config {
                FrontAuthenticationConfig::OIDCAuthorizationCodePKCEFlow {
                    user_profile_url,
                    ..
                } => Some(user_profile_url.clone()),
                FrontAuthenticationConfig::OIDCGoogleAuthorizationCodeFlow { user_profile_url } => {
                    Some(user_profile_url.clone())
                }
                FrontAuthenticationConfig::Local => None,
            });

    let connected_user_ref = use_atom_ref(cx, &CONNECTED_USER);

    let Some(user) = connected_user_ref.read().clone() else {
        return render! {
            div {
                class: "card w-full bg-base-200 text-base-content",
                div {
                    class: "h-full flex justify-center items-center",
                    Spinner {}
                    "Loading user profile..."
                }
            }
        };
    };

    let user_avatar = Gravatar::new(user.email.as_str())
        .set_size(Some(150))
        .set_rating(Some(Rating::G))
        .set_default(Some(gravatar::Default::MysteryMan))
        .image_url()
        .to_string();
    let user_name = format!("{} {}", user.first_name, user.last_name);

    render! {
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
                        render!(
                            a {
                                class: "btn btn-primary",
                                href: "{user_profile_url}",
                                target: "_blank",
                                rel: "noopener noreferrer",
                                "View detailed profile"
                            }
                        )
                    }
                }
            }
        }
    }
}
