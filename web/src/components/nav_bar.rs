use dioxus::prelude::*;
use dioxus_free_icons::icons::bs_icons::{BsGear, BsInbox, BsMoon, BsSun};
use dioxus_free_icons::Icon;
use dioxus_router::Link;
use log::debug;
use wasm_bindgen::JsValue;

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

fn toggle_dark_mode(toggle: bool) -> Result<bool, JsValue> {
    let window = web_sys::window().expect("Unable to get the window object");
    let document = window
        .document()
        .expect("Unable to get the document object");
    let document_element = document
        .document_element()
        .expect("Unable to get the document element");
    let local_storage = window
        .local_storage()?
        .expect("Unable to get the local storage");

    let dark_mode = match local_storage.get_item("color-theme") {
        Ok(Some(value)) if value == *"dark" => true,
        Ok(Some(_)) => false,
        _ => matches!(
            window.match_media("(prefers-color-scheme: dark)"),
            Ok(Some(_))
        ),
    };

    let switch_to_dark_mode = (dark_mode && !toggle) || (!dark_mode && toggle);
    debug!("Switching dark mode {switch_to_dark_mode}");
    if switch_to_dark_mode {
        document_element.set_attribute("data-theme", "dark")?;
        document_element.class_list().add_1("dark")?;
        local_storage.set_item("color-theme", "dark")?;
    } else {
        document_element.set_attribute("data-theme", "light")?;
        document_element.class_list().remove_1("dark")?;
        local_storage.set_item("color-theme", "light")?;
    }

    Ok(switch_to_dark_mode)
}
