#![allow(non_snake_case)]

use dioxus::prelude::dioxus_core::use_drop;
use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use dioxus_free_icons::{
    Icon,
    icons::{
        bs_icons::{
            BsBell, BsBook, BsBookmarkCheck, BsBoxArrowInLeft, BsGear, BsInbox, BsMoon, BsPerson,
            BsQuestionLg, BsSun,
        },
        go_icons::GoMarkGithub,
    },
};
use gravatar_rs::Generator;

use crate::{
    config::APP_CONFIG,
    icons::UILogo,
    model::{DEFAULT_USER_AVATAR, UI_MODEL, VERSION},
    route::Route,
    services::{
        crisp::init_crisp,
        flyonui::{forget_flyonui_dropdown_element, init_flyonui_dropdown_element},
        headway::init_headway,
        notification_service::NOTIFICATIONS_PAGE,
        task_service::SYNCED_TASKS_PAGE,
        user_service::{CONNECTED_USER, UserCommand},
    },
    theme::{IS_DARK_MODE, toggle_dark_mode},
};

pub fn NavBar() -> Element {
    let user_service = use_coroutine_handle::<UserCommand>();
    let user_avatar = use_memo(|| {
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
            .unwrap_or_else(|| DEFAULT_USER_AVATAR.to_string())
    });

    let show_changelog = APP_CONFIG
        .read()
        .as_ref()
        .map(|config| config.show_changelog)
        .unwrap_or_default();

    let mut mounted_element: Signal<Option<web_sys::Element>> = use_signal(|| None);

    use_drop(move || {
        if let Some(element) = mounted_element() {
            forget_flyonui_dropdown_element(&element);
        }
    });

    use_effect(move || {
        if show_changelog {
            init_headway();
        }
        if let Some(chat_support_website_id) = &APP_CONFIG
            .read()
            .as_ref()
            .and_then(|config| config.chat_support_website_id.clone())
        {
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
                Some(&user_avatar()),
                user_id.as_deref(),
            );
        }
    });

    rsx! {
        div {
            class: "navbar shadow-lg z-10 p-2 backdrop-blur-md border-b border-base-content/5",

            div {
                class: "sm:navbar-start max-sm:flex max-sm:grow items-center gap-8",

                UILogo { class: "rounded-full w-12 h-12" }

                div {
                    class: "indicator",
                    Link {
                        class: "btn btn-text px-2 min-h-10 h-10",
                        active_class: "btn-active",
                        to: Route::NotificationsPage {},
                        Icon { class: "w-5 h-5", icon: BsInbox }
                        p { "Inbox" }
                    }
                    if NOTIFICATIONS_PAGE().total > 0 {
                        span { class: "indicator-item indicator-top badge badge-sm badge-primary rounded-full text-xs", "{NOTIFICATIONS_PAGE().total}" }
                    }
                }

                div {
                    class: "indicator",
                    Link {
                        class: "btn btn-text px-2 min-h-10 h-10",
                        active_class: "btn-active",
                        to: Route::SyncedTasksPage {},
                        Icon { class: "w-5 h-5", icon: BsBookmarkCheck }
                        p { "Synced tasks" }
                    }
                    if SYNCED_TASKS_PAGE().total > 0 {
                        span { class: "indicator-item indicator-top badge badge-sm badge-primary rounded-full text-xs", "{SYNCED_TASKS_PAGE().total}" }
                    }
                }
            }

            if let Some(version) = VERSION {
                if UI_MODEL.read().is_help_enabled {
                    div {
                        class: "navbar-center max-sm:hidden",
                        span { class: "text-xs text-base-content/50", "build: {version}" }
                    }
                }
            }

            div {
                class: "sm:navbar-end items-center gap-2",

                NavBarUtils {
                    class: "max-sm:hidden",
                    show_changelog,
                    in_menu: false
                }

                div {
                    class: "dropdown relative inline-flex",
                    onmounted: move |element| {
                        let web_element = element.as_web_event();
                        init_flyonui_dropdown_element(&web_element);
                        mounted_element.set(Some(web_element));
                    },

                    button {
                        class: "btn btn-text btn-square dropdown-toggle",
                        "aria-haspopup": "menu",
                        "aria-expanded": "false",
                        "aria-label": "Dropdown",
                        type: "button",
                        tabindex: 0,

                        div {
                            class: "avatar w-8 h-8",

                            img {
                                class: "rounded-full",
                                src: "{user_avatar()}",
                            }
                        }
                    }

                    ul {
                        class: "mt-3 p-2 shadow-sm dropdown-menu dropdown-open:opacity-100 hidden rounded-box w-52",
                        role: "menu",
                        "aria-orientation": "vertical",
                        "aria-labelledby": "dropdown-menu-icon",
                        tabindex: 0,

                        NavBarUtils {
                            class: "sm:hidden",
                            show_changelog,
                            in_menu: true
                        }

                        li {
                            Link {
                                class: "dropdown-item",
                                to: Route::UserProfilePage {},
                                Icon { class: "w-5 h-5", icon: BsPerson }
                                p { "Profile" }
                            }
                        }
                        li {
                            a {
                                class: "dropdown-item",
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

#[component]
fn NavBarUtils(show_changelog: bool, in_menu: bool, class: Option<String>) -> Element {
    let support_href = APP_CONFIG
        .read()
        .as_ref()
        .and_then(|config| config.support_href.clone());
    let class = class.unwrap_or_default();

    rsx! {
        div {
            class: "flex {class}",

            a {
                class: "grow p-2 text-neutral",
                href: "https://github.com/universal-inbox/universal-inbox",
                title: "Universal Inbox on GitHub",
                target: "_blank",
                Icon { class: "w-5 h-5", icon: GoMarkGithub }
            }

            if show_changelog {
                button {
                    class: "btn btn-text btn-square grow relative",
                    div { id: "ui-changelog", class: "absolute top-0 left-0" }
                    Icon { class: "w-5 h-5", icon: BsBell }
                }
            }

            a {
                class: "btn btn-text btn-square grow",
                href: "https://doc.universal-inbox.com",
                title: "Universal Inbox documentation",
                target: "_blank",
                Icon { class: "w-5 h-5", icon: BsBook }
            }

            if let Some(support_href) = support_href {
                a {
                    class: "btn btn-text btn-square grow",
                    href: "{support_href}",
                    title: "Contact support",
                    Icon { class: "w-5 h-5", icon: BsQuestionLg }
                }
            }

            label {
                class: "btn btn-text btn-square swap swap-rotate grow",
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

            if !in_menu {
                Link {
                    class: "btn btn-text btn-square grow",
                    active_class: "btn-active",
                    to: Route::SettingsPage {},
                    Icon { class: "w-5 h-5", icon: BsGear }
                }
            }
        }

        if in_menu {
            li {
                Link {
                    class: "dropdown-item {class}",
                    to: Route::SettingsPage {},
                    Icon { class: "w-5 h-5", icon: BsGear }
                    p { "Settings" }
                }
            }
        }
    }
}
