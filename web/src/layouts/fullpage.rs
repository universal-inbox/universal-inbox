#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_router::hooks::use_route;
use gravatar_rs::Generator;

use crate::{
    components::{
        flyonui::tooltip::{Tooltip, TooltipPlacement},
        ui::{Button, ButtonSize, ButtonVariant},
        universal_inbox_title::BrandHeroLockup,
    },
    config::APP_CONFIG,
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

    // Clear any stale error / confirmation message when navigating between
    // auth pages (e.g. /login → /signup). `peek()` reads without subscribing,
    // so writing to last_route here doesn't loop.
    let route = use_route::<Route>();
    let mut last_route = use_signal(|| route.clone());
    if *last_route.peek() != route {
        last_route.set(route.clone());
        let mut model = UI_MODEL.write();
        model.error_message = None;
        model.confirmation_message = None;
    }

    use_effect(move || {
        if let Some(chat_support_website_id) = &APP_CONFIG
            .read()
            .as_ref()
            .and_then(|config| config.chat_support_website_id.clone())
        {
            let user_avatar = Some(
                CONNECTED_USER()
                    .as_ref()
                    .map(|user| {
                        if let Some(ref email) = user.email {
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
            let user_email = CONNECTED_USER()
                .as_ref()
                .and_then(|user| user.email.as_ref().map(|email| email.to_string()));
            let user_email_signature = CONNECTED_USER().as_ref().and_then(|user| {
                user.chat_support_email_signature
                    .as_ref()
                    .map(|signature| signature.to_string())
            });
            let user_full_name = CONNECTED_USER().as_ref().and_then(|user| user.full_name());
            let user_id = CONNECTED_USER().as_ref().map(|user| user.id.to_string());

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
            class: "flex min-h-screen items-center justify-center bg-ui-base-200 relative p-6",

            if CONNECTED_USER.read().is_some() {
                Tooltip {
                    text: "Logout",
                    placement: TooltipPlacement::Bottom,
                    button {
                        class: "absolute top-4 right-4 bg-transparent border border-ui-border rounded-ui-sm p-1.5 cursor-pointer text-ui-base-muted transition-colors hover:text-ui-base-content hover:border-ui-base-300",
                        onclick: move |_| user_service.send(UserCommand::Logout),
                        span { class: "icon-[lucide--log-out] size-5" }
                    }
                }
            }

            div { class: "auth-frame",
                div { class: "px-12 pt-12 pb-10 flex-1 flex flex-col bg-ui-surface",
                    BrandHeroLockup {}

                    div { class: "flex-1 flex flex-col justify-center min-h-0",

                        if let Some(error_message) = &UI_MODEL.read().error_message {
                            div {
                                id: "auth-error",
                                class: "flex gap-2 items-start px-3.5 py-2.5 rounded-ui-sm text-sm mb-3 bg-ui-error-subtle text-ui-error-hover",
                                role: "alert",
                                span { class: "icon-[lucide--alert-circle] size-4 shrink-0 mt-0.5" }
                                span { "{error_message}" }
                            }
                        }

                        if let Some(confirmation_message) = &UI_MODEL.read().confirmation_message {
                            div {
                                class: "flex flex-col items-center text-center py-3",

                                div { class: "w-14 h-14 rounded-full bg-ui-success-subtle text-ui-success flex items-center justify-center text-3xl mb-3.5",
                                    span { class: "icon-[lucide--check-circle] size-7" }
                                }
                                h1 { class: "font-ui text-2xl font-bold tracking-tight leading-tight text-ui-base-content mt-0 mb-1.5",
                                    "All set"
                                }
                                p { class: "text-sm text-ui-base-muted leading-normal mb-5",
                                    "{confirmation_message}"
                                }

                                Button {
                                    variant: ButtonVariant::Primary,
                                    size: ButtonSize::Md,
                                    class: "btn-block".to_string(),
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
}
