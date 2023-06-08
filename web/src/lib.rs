#[macro_use]
extern crate lazy_static;

use anyhow::{anyhow, Context, Result};
use dioxus::prelude::*;
use dioxus_router::{Route, Router};
use fermi::{use_atom_ref, use_init_atom_root, UseAtomRef};
use log::debug;
use utils::get_element_by_id;
use wasm_bindgen::{prelude::Closure, JsCast};
use web_sys::KeyboardEvent;

use universal_inbox::notification::NotificationWithTask;

use components::{footer::footer, nav_bar::nav_bar, spinner::spinner, toast_zone::toast_zone};
use config::{get_api_base_url, get_app_config, APP_CONFIG};
use model::{UniversalInboxUIModel, UI_MODEL};
use pages::{
    notifications_page::notifications_page, page_not_found::page_not_found,
    settings_page::settings_page,
};
use services::{
    integration_connection_service::{integration_connnection_service, INTEGRATION_CONNECTIONS},
    notification_service::{notification_service, NotificationCommand, NOTIFICATIONS},
    task_service::{task_service, TaskCommand},
    toast_service::{toast_service, TOASTS},
    user_service::{user_service, CONNECTED_USER},
};

mod auth;
mod components;
mod config;
mod model;
mod pages;
mod services;
mod theme;
mod utils;

pub fn app(cx: Scope) -> Element {
    use_init_atom_root(cx);
    let notifications_ref = use_atom_ref(cx, NOTIFICATIONS);
    let ui_model_ref = use_atom_ref(cx, UI_MODEL);
    let toasts_ref = use_atom_ref(cx, TOASTS);
    let app_config_ref = use_atom_ref(cx, APP_CONFIG);
    let connected_user_ref = use_atom_ref(cx, CONNECTED_USER);
    let integration_connections_ref = use_atom_ref(cx, INTEGRATION_CONNECTIONS);
    let api_base_url = use_memo(cx, (), |()| get_api_base_url().unwrap());
    let session_url = use_memo(cx, &(api_base_url.clone(),), |(api_base_url,)| {
        api_base_url.join("auth/session").unwrap()
    });

    let toast_service_handle = use_coroutine(cx, |rx| toast_service(rx, toasts_ref.clone()));
    let task_service_handle = use_coroutine(cx, |rx| {
        to_owned![toast_service_handle];

        task_service(
            rx,
            api_base_url.clone(),
            ui_model_ref.clone(),
            toast_service_handle,
        )
    });
    let notification_service_handle = use_coroutine(cx, |rx| {
        to_owned![toast_service_handle];
        to_owned![task_service_handle];

        notification_service(
            rx,
            api_base_url.clone(),
            notifications_ref.clone(),
            ui_model_ref.clone(),
            task_service_handle,
            toast_service_handle,
        )
    });
    let _user_service_handle = use_coroutine(cx, |rx| {
        user_service(
            rx,
            api_base_url.clone(),
            connected_user_ref.clone(),
            ui_model_ref.clone(),
        )
    });
    let _integration_connection_service_handle = use_coroutine(cx, |rx| {
        integration_connnection_service(
            rx,
            app_config_ref.clone(),
            integration_connections_ref.clone(),
            ui_model_ref.clone(),
            toast_service_handle.clone(),
            notification_service_handle.clone(),
        )
    });

    use_future(cx, (), |()| {
        to_owned![ui_model_ref];
        to_owned![notification_service_handle];
        to_owned![task_service_handle];
        to_owned![notifications_ref];
        to_owned![app_config_ref];

        async move {
            setup_key_bindings(
                ui_model_ref,
                notification_service_handle,
                task_service_handle,
                notifications_ref,
            );

            let app_config = get_app_config().await.unwrap();
            app_config_ref.write().replace(app_config);
        }
    });

    debug!("Rendering app");
    if let Some(app_config) = app_config_ref.read().as_ref() {
        cx.render(rsx!(
            // Router + Route == 300KB (release) !!!
            div {
                class: "h-full flex flex-col text-sm",

                Router {
                   auth::authenticated {
                       issuer_url: app_config.oidc_issuer_url.clone(),
                       client_id: app_config.oidc_client_id.clone(),
                       redirect_url: app_config.oidc_redirect_url.clone(),
                       session_url: session_url.clone(),
                       ui_model_ref: ui_model_ref.clone(),

                       self::nav_bar {}
                       Route { to: "/", self::notifications_page {} }
                       Route { to: "/settings", self::settings_page {} }
                       Route { to: "", self::page_not_found {} }
                       self::footer {}
                       self::toast_zone {}
                   }
                }
            }
        ))
    } else {
        cx.render(rsx!(div {
            class: "h-full flex justify-center items-center",

            self::spinner {}
            "Loading Universal Inbox..."
        }))
    }
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

        if ui_model_ref.read().task_planning_modal_opened
            || ui_model_ref.read().task_association_modal_opened
        {
            match evt.key().as_ref() {
                "Escape" => {
                    ui_model_ref.write().task_planning_modal_opened = false;
                    ui_model_ref.write().task_association_modal_opened = false;
                }
                _ => handled = false,
            }
        } else {
            match evt.key().as_ref() {
                "ArrowDown"
                    if ui_model_ref.read().selected_notification_index < (list_length - 1) =>
                {
                    let mut ui_model = ui_model_ref.write();
                    let new_selected_notification_index = ui_model.selected_notification_index + 1;
                    select_notification(&mut ui_model, new_selected_notification_index)
                        .unwrap_or_else(|_| {
                            panic!(
                                "Failed to select notification {new_selected_notification_index}"
                            )
                        });
                }
                "ArrowUp" if ui_model_ref.read().selected_notification_index > 0 => {
                    let mut ui_model = ui_model_ref.write();
                    let new_selected_notification_index = ui_model.selected_notification_index - 1;
                    select_notification(&mut ui_model, new_selected_notification_index)
                        .unwrap_or_else(|_| {
                            panic!(
                                "Failed to select notification {new_selected_notification_index}"
                            )
                        });
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
                "a" => ui_model_ref.write().task_association_modal_opened = true,
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

fn select_notification(ui_model: &mut UniversalInboxUIModel, index: usize) -> Result<()> {
    ui_model.selected_notification_index = index;
    let notification_page = get_element_by_id("notifications-page")
        .context("Unable to find `notifications-page` element")?;
    let row_height = notification_page
        .query_selector("tr")
        .map_err(|_| anyhow!("Unable to find a `tr` element in the notification page"))?
        .context("Unable to find a `tr` element in the notification page")?
        .client_height();
    let target_scroll = row_height * index as i32;
    notification_page.set_scroll_top(target_scroll);
    Ok(())
}
