use log::debug;

use dioxus::{
    core::to_owned,
    fermi::UseAtomRef,
    prelude::*,
    router::{Route, Router},
};
use wasm_bindgen::{prelude::Closure, JsCast};
use web_sys::KeyboardEvent;

use universal_inbox::notification::Notification;

use components::{footer::footer, nav_bar::nav_bar, toast_zone::toast_zone};
use pages::{
    notifications_page::notifications_page, page_not_found::page_not_found,
    settings_page::settings_page,
};
use services::{
    notification_service::{
        notification_service, NotificationCommand, UniversalInboxUIModel, NOTIFICATIONS, UI_MODEL,
    },
    task_service::{task_service, TaskCommand},
    toast_service::{toast_service, TOASTS},
};

mod components;
mod pages;
mod services;

pub fn app(cx: Scope) -> Element {
    let notifications_ref = use_atom_ref(&cx, NOTIFICATIONS);
    let ui_model_ref = use_atom_ref(&cx, UI_MODEL);
    let toasts_ref = use_atom_ref(&cx, TOASTS);
    let toast_service_handle = use_coroutine(&cx, |rx| toast_service(rx, toasts_ref.clone()));
    let task_service_handle = use_coroutine(&cx, |rx| {
        to_owned![toast_service_handle];

        task_service(rx, toast_service_handle)
    });
    let notification_service_handle = use_coroutine(&cx, |rx| {
        to_owned![toast_service_handle];
        to_owned![task_service_handle];

        notification_service(
            rx,
            notifications_ref.clone(),
            task_service_handle,
            toast_service_handle,
        )
    });

    use_future(&cx, (), |()| {
        to_owned![ui_model_ref];
        to_owned![notification_service_handle];
        to_owned![task_service_handle];
        to_owned![notifications_ref];

        async move {
            setup_key_bindings(
                ui_model_ref,
                notification_service_handle,
                task_service_handle,
                notifications_ref,
            );
        }
    });

    debug!("Rendering app");
    cx.render(rsx!(
        // Router + Route == 300KB (release) !!!
        div {
            class: "bg-light-0 dark:bg-dark-200 text-black dark:text-white h-full flex flex-col",

            Router {
                self::nav_bar {}
                Route { to: "/", self::notifications_page {} }
                Route { to: "/settings", self::settings_page {} }
                Route { to: "", self::page_not_found {} }
                self::footer {}
                self::toast_zone {}
            }
        }
    ))
}

fn setup_key_bindings(
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
    notification_service_handle: CoroutineHandle<NotificationCommand>,
    _task_service_handle: CoroutineHandle<TaskCommand>,
    notifications_ref: UseAtomRef<Vec<Notification>>,
) -> Option<()> {
    let window = web_sys::window()?;
    let document = window.document()?;

    let handler = Closure::wrap(Box::new(move |evt: KeyboardEvent| {
        let mut ui_model = ui_model_ref.write();
        let notifications = notifications_ref.read();
        let list_length = notifications.len();
        let selected_notification = notifications.get(ui_model.selected_notification_index);
        let mut handled = true;

        match evt.key().as_ref() {
            "ArrowDown" if ui_model.selected_notification_index < (list_length - 1) => {
                ui_model.selected_notification_index += 1
            }
            "ArrowUp" if ui_model.selected_notification_index > 0 => {
                ui_model.selected_notification_index -= 1
            }
            "d" => {
                if let Some(notification) = selected_notification {
                    notification_service_handle.send(NotificationCommand::DeleteFromNotification(
                        notification.clone(),
                    ))
                }
            }
            "u" => {
                if let Some(notification) = selected_notification {
                    notification_service_handle
                        .send(NotificationCommand::Unsubscribe(notification.id))
                }
            }
            "s" => {
                if let Some(notification) = selected_notification {
                    notification_service_handle.send(NotificationCommand::Snooze(notification.id))
                }
            }
            "h" => ui_model.footer_help_opened = !ui_model.footer_help_opened,
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
