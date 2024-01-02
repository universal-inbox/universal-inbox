#![allow(non_snake_case)]

#[macro_use]
extern crate lazy_static;

use anyhow::{anyhow, Context, Result};
use dioxus::prelude::*;
use dioxus_router::prelude::*;
use fermi::{use_atom_ref, use_atom_state, use_init_atom_root, UseAtomRef};
use gloo_utils::errors::JsError;
use log::debug;
use utils::get_element_by_id;
use wasm_bindgen::{prelude::Closure, JsCast};
use web_sys::KeyboardEvent;

use universal_inbox::notification::NotificationWithTask;

use config::{get_api_base_url, get_app_config, APP_CONFIG};
use model::{PreviewPane, UniversalInboxUIModel, UI_MODEL};
use route::Route;
use services::{
    integration_connection_service::{integration_connnection_service, INTEGRATION_CONNECTIONS},
    notification_service::{notification_service, NotificationCommand, NOTIFICATIONS},
    task_service::{task_service, TaskCommand},
    toast_service::{toast_service, TOASTS},
    user_service::{user_service, CONNECTED_USER},
};

use crate::{
    theme::{toggle_dark_mode, IS_DARK_MODE},
    utils::{current_location, get_local_storage},
};

mod auth;
mod components;
mod config;
mod form;
mod layouts;
mod model;
mod pages;
mod route;
mod services;
mod theme;
mod utils;

pub fn App(cx: Scope) -> Element {
    use_init_atom_root(cx);
    let notifications_ref = use_atom_ref(cx, &NOTIFICATIONS);
    let ui_model_ref: UseAtomRef<UniversalInboxUIModel> = use_atom_ref(cx, &UI_MODEL).clone();
    let toasts_ref = use_atom_ref(cx, &TOASTS);
    let app_config_ref = use_atom_ref(cx, &APP_CONFIG);
    let connected_user_ref = use_atom_ref(cx, &CONNECTED_USER);
    let integration_connections_ref = use_atom_ref(cx, &INTEGRATION_CONNECTIONS);
    let api_base_url = use_memo(cx, (), |()| get_api_base_url().unwrap());

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
            task_service_handle.clone(),
        )
    });
    let is_dark_mode = use_atom_state(cx, &IS_DARK_MODE);

    use_future(cx, (), |()| {
        to_owned![ui_model_ref];
        to_owned![notification_service_handle];
        to_owned![task_service_handle];
        to_owned![notifications_ref];
        to_owned![app_config_ref];
        to_owned![is_dark_mode];

        async move {
            is_dark_mode.set(toggle_dark_mode(false).expect("Failed to initialize the theme"));

            setup_key_bindings(
                ui_model_ref,
                notification_service_handle.clone(),
                task_service_handle,
                notifications_ref,
            );

            let app_config = get_app_config().await.unwrap();
            app_config_ref.write().replace(app_config);
        }
    });

    // Dioxus 0.4.1 bug workaround: https://github.com/DioxusLabs/dioxus/issues/1511
    let current_url = current_location().unwrap();
    let auth_code = current_url
        .query_pairs()
        .find(|(k, _)| k == "code")
        .map(|(_, v)| v.to_string());
    let local_storage = get_local_storage().unwrap();
    if let Some(auth_code) = auth_code {
        debug!("auth: Storing auth-oidc-callback-code {auth_code:?}");
        local_storage
            .set_item("auth-oidc-callback-code", &auth_code)
            .map_err(|err| JsError::try_from(err).unwrap())
            .unwrap();
    }
    // end workaround

    debug!("Rendering app");
    render! {
        Router::<Route> {
            config: move || {
                RouterConfig::default()
                    .history(WebHistory::<Route>::default())
                    .on_update(move |_state| {
                        ui_model_ref.clone().write().error_message = None;
                        ui_model_ref.clone().write().confirmation_message = None;
                        None
                    })
            }
        }
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
        let current_url = current_location().unwrap();
        let mut handled = true;

        if current_url.path() != "/" {
            // The notification page
            handled = false;
        } else if ui_model_ref.read().task_planning_modal_opened
            || ui_model_ref.read().task_link_modal_opened
        {
            match evt.key().as_ref() {
                "Escape" => {
                    ui_model_ref.write().task_planning_modal_opened = false;
                    ui_model_ref.write().task_link_modal_opened = false;
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
                "ArrowRight"
                    if ui_model_ref.read().selected_preview_pane == PreviewPane::Notification
                        && selected_notification
                            .map(|notif| notif.task.is_some())
                            .unwrap_or_default() =>
                {
                    ui_model_ref.write().selected_preview_pane = PreviewPane::Task;
                }
                "ArrowLeft"
                    if ui_model_ref.read().selected_preview_pane == PreviewPane::Task
                        && !selected_notification
                            .map(|notif| notif.is_built_from_task())
                            .unwrap_or_default() =>
                {
                    ui_model_ref.write().selected_preview_pane = PreviewPane::Notification;
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
                "l" => ui_model_ref.write().task_link_modal_opened = true,
                "h" | "?" => ui_model_ref.write().toggle_help(),
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
    let notifications_list = get_element_by_id("notifications-list")
        .context("Unable to find `notifications-list` element")?;
    let row_height = notifications_list
        .query_selector("tr")
        .map_err(|_| anyhow!("Unable to find a `tr` element in the notification page"))?
        .context("Unable to find a `tr` element in the notification page")?
        .client_height();
    let target_scroll = row_height * index as i32;
    notifications_list.set_scroll_top(target_scroll);
    Ok(())
}
