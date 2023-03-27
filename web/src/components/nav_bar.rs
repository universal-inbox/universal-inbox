use dioxus::prelude::*;
use dioxus_free_icons::icons::bs_icons::{BsGear, BsInbox, BsMoon, BsSun};
use dioxus_free_icons::Icon;
use dioxus_router::Link;

use crate::theme::toggle_dark_mode;

pub fn nav_bar(cx: Scope) -> Element {
    let is_dark_mode = use_state(cx, || {
        toggle_dark_mode(false).expect("Failed to initialize the theme")
    });

    cx.render(rsx! {
        div {
            class: "navbar shadow-lg z-10",

            div {
                class: "navbar-start",

                Link {
                    class: "btn btn-ghost gap-2",
                    active_class: "btn-active",
                    to: "/",
                    Icon { class: "w-5 h-5", icon: BsInbox }
                    p { "Inbox" }
                }
            }

            div {
                class: "navbar-end",

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
                    to: "/settings",
                    title: "Settings",
                    Icon { class: "w-5 h-5", icon: BsGear }
                }
            }
        }
    })
}
