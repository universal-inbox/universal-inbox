#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::bs_icons::{BsBoxArrowInLeft, BsCheckCircle},
};
use gravatar_rs::Generator;

use crate::{
    config::APP_CONFIG,
    icons::UILogo,
    model::{DEFAULT_USER_AVATAR, UI_MODEL},
    route::Route,
    services::{
        crisp::init_crisp,
        user_service::{CONNECTED_USER, UserCommand},
    },
};

#[component]
pub fn FullpageLayout() -> Element {
    let user_service = use_coroutine_handle::<UserCommand>();
    let nav = use_navigator();

    use_effect(move || {
        if let Some(chat_support_website_id) = &APP_CONFIG
            .read()
            .as_ref()
            .and_then(|config| config.chat_support_website_id.clone())
        {
            let user_avatar = Some(
                CONNECTED_USER()
                    .as_ref()
                    .map(|user_context| {
                        if let Some(ref email) = user_context.user.email {
                            Generator::default()
                                .set_image_size(150)
                                .set_rating("g")
                                .set_default_image("mp")
                                .generate(email.as_str())
                        } else {
                            DEFAULT_USER_AVATAR.to_string()
                        }
                    })
                    .unwrap_or_else(|| DEFAULT_USER_AVATAR.to_string()),
            );
            let user_email = CONNECTED_USER().as_ref().and_then(|user_context| {
                user_context
                    .user
                    .email
                    .as_ref()
                    .map(|email| email.to_string())
            });
            let user_email_signature = CONNECTED_USER().as_ref().and_then(|user_context| {
                user_context
                    .user
                    .chat_support_email_signature
                    .as_ref()
                    .map(|signature| signature.to_string())
            });
            let user_full_name = CONNECTED_USER()
                .as_ref()
                .and_then(|user_context| user_context.user.full_name());
            let user_id = CONNECTED_USER()
                .as_ref()
                .map(|user_context| user_context.user.id.to_string());

            init_crisp(
                chat_support_website_id,
                user_email.as_deref(),
                user_email_signature.as_deref(),
                user_full_name.as_deref(),
                user_avatar.as_deref(),
                user_id.as_deref(),
            );
        }
    });

    rsx! {
        div {
            class: "flex min-h-screen items-center justify-center bg-base-100 relative",

            if CONNECTED_USER.read().is_some() {
                button {
                    class: "btn btn-text absolute top-4 right-4",
                    "data-tip": "Logout",
                    onclick: move |_| user_service.send(UserCommand::Logout),
                    Icon { class: "w-5 h-5", icon: BsBoxArrowInLeft }
                }
            }

            div {
                class: "m-4 min-h-[50vh] w-full max-w-md",

                main {
                    div {
                        class: "flex flex-col items-center justify-center",
                        UILogo { class: "rounded-full w-48 h-48" }
                    }

                    if let Some(error_message) = &UI_MODEL.read().error_message {
                        div {
                            class: "alert alert-error text-sm flex gap-2",
                            role: "alert",
                            "{error_message}"
                        }
                    }

                    if let Some(confirmation_message) = &UI_MODEL.read().confirmation_message {
                        div {
                            class: "flex flex-col items-center justify-center gap-10",

                            div {
                                class: "alert alert-success text-sm flex gap-2",
                                role: "alert",
                                Icon { class: "w-5 h-5", icon: BsCheckCircle }
                                "{confirmation_message}"
                            }

                            button {
                                class: "btn btn-primary mt-2",
                                onclick: move |_| {
                                    UI_MODEL.write().confirmation_message = None;
                                    UI_MODEL.write().error_message = None;
                                    nav.push(Route::LoginPage {});
                                },
                                "Return to Universal Inbox"
                            }
                        }
                    } else {
                        Outlet::<Route> {}
                    }
                }
            }
        }
    }
}
