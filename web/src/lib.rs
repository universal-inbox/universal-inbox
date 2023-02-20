use dioxus::prelude::*;
use dioxus_router::{Route, Router};
use fermi::{use_atom_ref, use_init_atom_root, UseAtomRef};
use log::debug;
use wasm_bindgen::{prelude::Closure, JsCast};
use web_sys::KeyboardEvent;

use universal_inbox::notification::NotificationWithTask;

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
mod utils;

pub fn app(cx: Scope) -> Element {
    use_init_atom_root(cx);
    let notifications_ref = use_atom_ref(cx, NOTIFICATIONS);
    let ui_model_ref = use_atom_ref(cx, UI_MODEL);
    let toasts_ref = use_atom_ref(cx, TOASTS);
    let toast_service_handle = use_coroutine(cx, |rx| toast_service(rx, toasts_ref.clone()));
    let task_service_handle = use_coroutine(cx, |rx| {
        to_owned![toast_service_handle];

        task_service(rx, toast_service_handle)
    });
    let notification_service_handle = use_coroutine(cx, |rx| {
        to_owned![toast_service_handle];
        to_owned![task_service_handle];

        notification_service(
            rx,
            notifications_ref.clone(),
            task_service_handle,
            toast_service_handle,
        )
    });

    use_future(cx, (), |()| {
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
            class: "h-full flex flex-col",

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
    notification_service_handle: Coroutine<NotificationCommand>,
    _task_service_handle: Coroutine<TaskCommand>,
    notifications_ref: UseAtomRef<Vec<NotificationWithTask>>,
) -> Option<()> {
    let window = web_sys::window()?;
    let document = window.document()?;

    let handler = Closure::wrap(Box::new(move |evt: KeyboardEvent| {
        let notifications = notifications_ref.read();
        let list_length = notifications.len();
        let selected_notification =
            notifications.get(ui_model_ref.read().selected_notification_index);
        let mut handled = true;

        if ui_model_ref.read().task_planning_modal_opened {
            match evt.key().as_ref() {
                "Escape" => ui_model_ref.write().task_planning_modal_opened = false,
                _ => handled = false,
            }
        } else {
            match evt.key().as_ref() {
                "ArrowDown"
                    if ui_model_ref.read().selected_notification_index < (list_length - 1) =>
                {
                    ui_model_ref.write().selected_notification_index += 1
                }
                "ArrowUp" if ui_model_ref.read().selected_notification_index > 0 => {
                    ui_model_ref.write().selected_notification_index -= 1
                }
                "d" => {
                    if let Some(notification) = selected_notification {
                        notification_service_handle.send(
                            NotificationCommand::DeleteFromNotification(notification.clone()),
                        )
                    }
                }
                "c" => {
                    if let Some(notification) = selected_notification {
                        notification_service_handle.send(
                            NotificationCommand::CompleteTaskFromNotification(notification.clone()),
                        )
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
                        notification_service_handle
                            .send(NotificationCommand::Snooze(notification.id))
                    }
                }
                "p" => ui_model_ref.write().task_planning_modal_opened = true,
                "h" => ui_model_ref.write().toggle_help(),
                _ => handled = false,
            }
        }

        if handled {
            ui_model_ref.write().set_unhover_element(true);
            evt.prevent_default();
        }
    }) as Box<dyn FnMut(KeyboardEvent)>);

    document
        .add_event_listener_with_callback("keydown", handler.as_ref().unchecked_ref())
        .expect("Failed to add `keydown` event listener");
    handler.forget();

    Some(())
}
