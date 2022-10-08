use components::nav_bar::nav_bar;
use dioxus::{
    core::to_owned,
    fermi::UseAtomRef,
    prelude::*,
    router::{Route, Router},
};
use log::debug;
use pages::notifications_page::notifications_page;
use pages::page_not_found::page_not_found;
use pages::settings_page::settings_page;
use wasm_bindgen::prelude::Closure;
use wasm_bindgen::JsCast;
use web_sys::KeyboardEvent;

use crate::services::notification_service::{
    notification_service, NOTIFICATIONS, SELECTED_NOTIFICATION_INDEX,
};

mod components;
mod pages;
mod services;

pub fn app(cx: Scope) -> Element {
    let notifications = use_atom_ref(&cx, NOTIFICATIONS);
    let selected_notification_index = use_atom_ref(&cx, SELECTED_NOTIFICATION_INDEX);
    use_coroutine(&cx, |rx| notification_service(rx, notifications.clone()));

    use_future(&cx, (), |()| {
        to_owned![selected_notification_index];
        async move {
            setup_key_bindings(selected_notification_index);
        }
    });

    debug!("Rendering app");
    cx.render(rsx!(
        // Router + Route == 300KB (release) !!!
        div {
            class: "bg-light-200 dark:bg-dark-900 text-black dark:text-white h-fit",

            Router {
                self::nav_bar {}
                Route { to: "/",
                        self::notifications_page {}
                }
                Route { to: "/settings", self::settings_page {} }
                Route { to: "", self::page_not_found {} }
            }
        }
    ))
}

fn setup_key_bindings(selected_notification_index: UseAtomRef<usize>) -> Option<()> {
    let window = web_sys::window()?;
    let document = window.document()?;

    let handler = Closure::wrap(Box::new(move |evt: KeyboardEvent| {
        evt.prevent_default();
        let mut index = selected_notification_index.write();
        if evt.key() == *"ArrowDown" {
            *index += 1;
        } else if evt.key() == *"ArrowUp" && *index > 0 {
            *index -= 1;
        }
    }) as Box<dyn FnMut(KeyboardEvent)>);

    document
        .add_event_listener_with_callback("keydown", handler.as_ref().unchecked_ref())
        .expect("Failed to add `keydown` event listener");
    handler.forget();

    Some(())
}
