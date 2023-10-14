#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::icons::bs_icons::{
    BsBoxArrowInLeft, BsGear, BsInbox, BsMoon, BsPerson, BsQuestionLg, BsSun,
};
use dioxus_free_icons::Icon;
use dioxus_router::prelude::*;
use fermi::use_atom_ref;
use gravatar::{Gravatar, Rating};

use crate::{
    config::APP_CONFIG,
    route::Route,
    services::user_service::{UserCommand, CONNECTED_USER},
    theme::toggle_dark_mode,
};

const DEFAULT_USER_AVATAR: &str = "https://avatars.githubusercontent.com/u/1062408?v=4";
const NOT_CONNECTED_USER_NAME: &str = "Not connected";

pub fn NavBar(cx: Scope) -> Element {
    let user_service = use_coroutine_handle::<UserCommand>(cx).unwrap();
    let connected_user_ref = use_atom_ref(cx, &CONNECTED_USER);
    let user_avatar = use_memo(cx, &connected_user_ref.read().clone(), |connected_user| {
        connected_user
            .map(|user| {
                Gravatar::new(user.email.as_str())
                    .set_size(Some(150))
                    .set_rating(Some(Rating::G))
                    .set_default(Some(gravatar::Default::MysteryMan))
                    .image_url()
                    .to_string()
            })
            .unwrap_or_else(|| DEFAULT_USER_AVATAR.to_string())
    });
    let user_name = use_memo(cx, &connected_user_ref.read().clone(), |connected_user| {
        connected_user
            .map(|user| user.first_name)
            .unwrap_or_else(|| NOT_CONNECTED_USER_NAME.to_string())
    });

    let app_config_ref = use_atom_ref(cx, &APP_CONFIG);
    // Howto use use_memo with an Option?
    let user_profile_url = app_config_ref
        .read()
        .as_ref()
        .map(|config| config.user_profile_url.clone());

    let is_dark_mode = use_state(cx, || {
        toggle_dark_mode(false).expect("Failed to initialize the theme")
    });

    use_future(cx, (), |()| {
        to_owned![user_service];
        async move {
            user_service.send(UserCommand::GetUser);
        }
    });

    render! {
        div {
            class: "navbar shadow-lg z-10",

            div {
                class: "navbar-start",

                img {
                    class: "rounded-full w-12 h-12",
                    src: "images/ui-logo-transparent.png",
                    alt: "Universal Inbox logo",
                }
                Link {
                    class: "btn btn-ghost gap-2",
                    active_class: "btn-active",
                    to: Route::NotificationsPage {},
                    Icon { class: "w-5 h-5", icon: BsInbox }
                    p { "Inbox" }
                }
            }

            div {
                class: "navbar-end",

                a {
                    class: "btn btn-ghost btn-square",
                    href: "mailto:support@universal-inbox.com",
                    title: "Contact support",
                    Icon { class: "w-5 h-5", icon: BsQuestionLg }
                }

                label {
                    class: "btn btn-ghost btn-square swap swap-rotate",
                    input {
                        class: "hidden",
                        "type": "checkbox",
                        checked: "{is_dark_mode}",
                        onclick: |_| {
                            is_dark_mode.set(
                                toggle_dark_mode(true)
                                    .expect("Failed to switch the theme")
                            );
                        }
                    }
                    Icon { class: "swap-on w-5 h-5", icon: BsSun }
                    Icon { class: "swap-off w-5 h-5", icon: BsMoon }
                }

                Link {
                    class: "btn btn-ghost btn-square",
                    active_class: "btn-active",
                    to: Route::SettingsPage {},
                    Icon { class: "w-5 h-5", icon: BsGear }
                }

                div {
                    class: "dropdown dropdown-end",

                    label {
                        class: "btn btn-ghost btn-square avatar",
                        tabindex: 0,

                        div {
                            class: "rounded-full w-8 h-8",
                            title: "{user_name}",

                            img {
                                class: "",
                                src: "{user_avatar}",
                                alt: "{user_name}",
                            }
                        }
                    }

                    ul {
                        class: "mt-3 p-2 shadow menu dropdown-content bg-base-100 rounded-box w-52",
                        tabindex: 0,

                        if let Some(user_profile_url) = user_profile_url {
                            render! {
                                li {
                                    a {
                                        href: "{user_profile_url}",
                                        target: "_blank",
                                        Icon { class: "w-5 h-5", icon: BsPerson }
                                        p { "Profile" }
                                    }
                                }
                            }
                        }
                        li {
                            a {
                                onclick: |_| user_service.send(UserCommand::Logout),
                                Icon { class: "w-5 h-5", icon: BsBoxArrowInLeft }
                                p { "Logout" }
                            }
                        }
                    }
                }
            }
        }
    }
}
