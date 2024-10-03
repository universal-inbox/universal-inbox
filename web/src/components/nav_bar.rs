#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::{
        bs_icons::{
            BsBell, BsBookmarkCheck, BsBoxArrowInLeft, BsGear, BsInbox, BsMoon, BsPerson,
            BsQuestionLg, BsSun,
        },
        go_icons::GoMarkGithub,
    },
    Icon,
};
use gravatar::{Gravatar, Rating};

use crate::{
    config::APP_CONFIG,
    model::{DEFAULT_USER_AVATAR, NOT_CONNECTED_USER_NAME},
    route::Route,
    services::{
        notification_service::NOTIFICATIONS_PAGE,
        task_service::SYNCED_TASKS_PAGE,
        user_service::{UserCommand, CONNECTED_USER},
    },
    theme::{toggle_dark_mode, IS_DARK_MODE},
};

pub fn NavBar() -> Element {
    let user_service = use_coroutine_handle::<UserCommand>();
    let connected_user = CONNECTED_USER.read();
    let user_avatar = connected_user
        .as_ref()
        .map(|user| {
            Gravatar::new(user.email.as_str())
                .set_size(Some(150))
                .set_rating(Some(Rating::G))
                .set_default(Some(gravatar::Default::MysteryMan))
                .image_url()
                .to_string()
        })
        .unwrap_or_else(|| DEFAULT_USER_AVATAR.to_string());
    let user_name = connected_user
        .as_ref()
        .map(|user| user.first_name.clone())
        .unwrap_or_else(|| NOT_CONNECTED_USER_NAME.to_string());

    let support_href = APP_CONFIG
        .read()
        .as_ref()
        .and_then(|config| config.support_href.clone());
    let show_changelog = APP_CONFIG
        .read()
        .as_ref()
        .map(|config| config.show_changelog)
        .unwrap_or_default();

    rsx! {
        div {
            class: "navbar shadow-lg z-10 h-12",

            div {
                class: "navbar-start",

                img {
                    class: "rounded-full w-12 h-12",
                    src: "images/ui-logo-transparent.png",
                    alt: "Universal Inbox logo",
                }

                div {
                    class: "indicator mx-4",
                    Link {
                        class: "btn btn-ghost px-2 min-h-10 h-10",
                        active_class: "btn-active",
                        to: Route::NotificationsPage {},
                        Icon { class: "w-5 h-5", icon: BsInbox }
                        p { "Inbox" }
                    }
                    if NOTIFICATIONS_PAGE().total > 0 {
                      span { class: "indicator-item indicator-top badge badge-primary text-xs", "{NOTIFICATIONS_PAGE().total}" }
                    }
                }

                div {
                    class: "indicator mx-4",
                    Link {
                        class: "btn btn-ghost px-2 min-h-10 h-10",
                        active_class: "btn-active",
                        to: Route::SyncedTasksPage {},
                        Icon { class: "w-5 h-5", icon: BsBookmarkCheck }
                        p { "Synced tasks" }
                    }
                    if SYNCED_TASKS_PAGE().total > 0 {
                      span { class: "indicator-item indicator-top badge badge-primary text-xs", "{SYNCED_TASKS_PAGE().total}" }
                    }
                }
            }

            div {
                class: "navbar-end",

                a {
                    class: "p-2",
                    href: "https://github.com/universal-inbox/universal-inbox",
                    title: "Universal Inbox on GitHub",
                    target: "_blank",
                    Icon { class: "w-5 h-5", icon: GoMarkGithub }
                }

                if show_changelog {
                    button {
                       class: "btn btn-ghost btn-square relative",
                       div { id: "ui-changelog", class: "absolute top-0 left-0" }
                       Icon { class: "w-5 h-5", icon: BsBell }
                    }
                }

                if let Some(support_href) = support_href {
                    a {
                        class: "btn btn-ghost btn-square",
                        href: "{support_href}",
                        title: "Contact support",
                        Icon { class: "w-5 h-5", icon: BsQuestionLg }
                    }
                }

                label {
                    class: "btn btn-ghost btn-square swap swap-rotate",
                    input {
                        class: "hidden",
                        "type": "checkbox",
                        checked: "{IS_DARK_MODE}",
                        onclick: move |_| {
                            *IS_DARK_MODE.write() = toggle_dark_mode(true).expect("Failed to switch the theme");
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

                        li {
                            Link {
                                to: Route::UserProfilePage {},
                                Icon { class: "w-5 h-5", icon: BsPerson }
                                p { "Profile" }
                            }
                        }
                        li {
                            a {
                                onclick: move |_| user_service.send(UserCommand::Logout),
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
