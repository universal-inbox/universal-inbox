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
use services::notification_service::NotificationCommand;
use universal_inbox::Notification;
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
    let notification_service_handle =
        use_coroutine(&cx, |rx| notification_service(rx, notifications.clone()));

    use_future(&cx, (), |()| {
        to_owned![selected_notification_index];
        to_owned![notification_service_handle];
        to_owned![notifications];
        async move {
            setup_key_bindings(
                selected_notification_index,
                notification_service_handle,
                notifications,
            );
        }
    });

    debug!("Rendering app");
    cx.render(rsx!(
        // Router + Route == 300KB (release) !!!
        div {
            class: "bg-light-200 dark:bg-dark-900 text-black dark:text-white min-h-screen",

            Router {
                self::nav_bar {}
                Route { to: "/", self::notifications_page {} }
                Route { to: "/settings", self::settings_page {} }
                Route { to: "", self::page_not_found {} }
            }
        }
    ))
}

fn setup_key_bindings(
    selected_notification_index: UseAtomRef<usize>,
    notification_service_handle: CoroutineHandle<NotificationCommand>,
    notifications: UseAtomRef<Vec<Notification>>,
) -> Option<()> {
    let window = web_sys::window()?;
    let document = window.document()?;

    let handler = Closure::wrap(Box::new(move |evt: KeyboardEvent| {
        let mut index = selected_notification_index.write();
        let read_notifications = notifications.read();
        let list_length = read_notifications.len();
        let selected_notification = read_notifications.get(*index);
        let mut handled = true;

        match evt.key().as_ref() {
            "ArrowDown" if *index < (list_length - 1) => *index += 1,
            "ArrowUp" if *index > 0 => *index -= 1,
            "d" => {
                if let Some(notification) = selected_notification {
                    notification_service_handle
                        .send(NotificationCommand::MarkAsDone(notification.clone()))
                }
            }
            _ => handled = false,
        }
        if handled {
            evt.prevent_default();
        }
    }) as Box<dyn FnMut(KeyboardEvent)>);

    document
        .add_event_listener_with_callback("keydown", handler.as_ref().unchecked_ref())
        .expect("Failed to add `keydown` event listener");
    handler.forget();

    Some(())
}
