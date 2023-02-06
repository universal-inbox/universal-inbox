use dioxus::prelude::*;
use dioxus_free_icons::icons::bs_icons::{BsGear, BsInbox, BsMoon, BsSun};
use dioxus_free_icons::Icon;
use dioxus_router::Link;
use log::debug;
use wasm_bindgen::JsValue;

pub fn nav_bar(cx: Scope) -> Element {
    use_future(cx, (), |()| async move {
        toggle_dark_mode(false).expect("Failed to initialize the theme");
    });

    cx.render(rsx! {
        div {
            class: "container mx-auto px-4 bg-light-0 dark:bg-dark-200",
            div {
                class: "flex items-center",
                div {
                    class: "flex flex-none h-14 items-center justify-center",
                    Link {
                        class: "flex gap-2 bg-light-0 hover:bg-light-400 hover:shadow-md dark:bg-dark-200 dark:hover:bg-dark-600 text-sm p-2.5",
                        active_class: "flex gap-2 bg-light-200 hover:bg-light-400 hover:shadow-md dark:bg-dark-300 dark:hover:bg-dark-600 text-sm p-2.5",
                        to: "/",
                        Icon { class: "w-5 h-5", icon: BsInbox }
                        p { "Inbox" }
                    }
                }
                div { class: "grow" }
                div {
                    class: "flex flex-none h-14 w-14 items-center justify-center",
                    button {
                        id: "theme-toggle",
                        "type": "button",
                        onclick: |_| { toggle_dark_mode(true).expect("Failed to switch the theme"); },
                        class: "bg-light-0 hover:bg-light-400 hover:shadow-md dark:bg-dark-200 dark:hover:bg-dark-600 text-sm p-2.5",
                        title: "Switch lightning theme",

                        Icon { class: "block dark:hidden w-5 h-5", icon: BsMoon }
                        Icon { class: "hidden dark:block w-5 h-5", icon: BsSun }
                    }
                }
                div {
                    class: "flex flex-none h-14 w-14 items-center justify-center",
                    Link {
                        class: "bg-light-0 hover:bg-light-400 hover:shadow-md dark:bg-dark-200 dark:hover:bg-dark-600 text-sm p-2.5",
                        active_class: "bg-light-200 hover:bg-light-400 hover:shadow-md dark:bg-dark-300 dark:hover:bg-dark-600 text-sm p-2.5",
                        to: "/settings",
                        title: "Settings",
                        Icon { class: "w-5 h-5", icon: BsGear }
                    }
                }
            }
        }
    })
}

fn toggle_dark_mode(toggle: bool) -> Result<(), JsValue> {
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
        document_element.class_list().add_1("dark")?;
        local_storage.set_item("color-theme", "dark")?;
    } else {
        document_element.class_list().remove_1("dark")?;
        local_storage.set_item("color-theme", "light")?;
    }

    Ok(())
}
