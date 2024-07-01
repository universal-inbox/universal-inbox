#![allow(non_snake_case)]

#[macro_use]
extern crate lazy_static;

use anyhow::{anyhow, Context, Result};
use dioxus::prelude::*;

use gloo_utils::errors::JsError;
use log::debug;
use utils::get_element_by_id;
use wasm_bindgen::{prelude::Closure, JsCast};
use web_sys::KeyboardEvent;

use universal_inbox::{notification::NotificationWithTask, HasHtmlUrl, Page};

use config::{get_api_base_url, get_app_config, APP_CONFIG};
use model::{PreviewPane, UniversalInboxUIModel, UI_MODEL};
use route::Route;
use services::{
    authentication_token_service::{
        authentication_token_service, AUTHENTICATION_TOKENS, CREATED_AUTHENTICATION_TOKEN,
    },
    integration_connection_service::{integration_connnection_service, INTEGRATION_CONNECTIONS},
    notification_service::{notification_service, NotificationCommand, NOTIFICATIONS_PAGE},
    task_service::{task_service, TaskCommand},
    toast_service::{toast_service, TOASTS},
    user_service::{user_service, CONNECTED_USER},
};
use theme::{toggle_dark_mode, IS_DARK_MODE};
use utils::{current_location, get_local_storage, open_link};

use crate::services::{
    headway::init_headway, integration_connection_service::TASK_SERVICE_INTEGRATION_CONNECTION,
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

pub fn App() -> Element {
    let api_base_url = use_memo(move || get_api_base_url().unwrap());

    let toast_service_handle = use_coroutine(|rx| toast_service(rx, TOASTS.signal()));
    let task_service_handle = use_coroutine(|rx| {
        to_owned![toast_service_handle];

        task_service(rx, api_base_url(), UI_MODEL.signal(), toast_service_handle)
    });
    let notification_service_handle = use_coroutine(|rx| {
        to_owned![toast_service_handle];
        to_owned![task_service_handle];

        notification_service(
            rx,
            api_base_url(),
            NOTIFICATIONS_PAGE.signal(),
            UI_MODEL.signal(),
            task_service_handle,
            toast_service_handle,
        )
    });
    let _user_service_handle = use_coroutine(|rx| {
        user_service(
            rx,
            api_base_url(),
            CONNECTED_USER.signal(),
            UI_MODEL.signal(),
        )
    });
    let _integration_connection_service_handle = use_coroutine(|rx| {
        integration_connnection_service(
            rx,
            APP_CONFIG.signal().into(),
            INTEGRATION_CONNECTIONS.signal(),
            TASK_SERVICE_INTEGRATION_CONNECTION.signal(),
            UI_MODEL.signal(),
            toast_service_handle,
            notification_service_handle,
            task_service_handle,
        )
    });

    let _authentication_token_service_handle = use_coroutine(|rx| {
        authentication_token_service(
            rx,
            api_base_url(),
            AUTHENTICATION_TOKENS.signal(),
            CREATED_AUTHENTICATION_TOKEN.signal(),
            UI_MODEL.signal(),
            toast_service_handle,
        )
    });

    let show_changelog = APP_CONFIG
        .read()
        .as_ref()
        .map(|config| config.show_changelog)
        .unwrap_or_default();

    if show_changelog {
        init_headway();
    }

    use_future(move || {
        to_owned![notification_service_handle];
        to_owned![task_service_handle];

        async move {
            *IS_DARK_MODE.write() =
                toggle_dark_mode(false).expect("Failed to initialize the theme");

            setup_key_bindings(
                UI_MODEL.signal(),
                notification_service_handle,
                task_service_handle,
                NOTIFICATIONS_PAGE.signal().into(),
            );

            let app_config = get_app_config().await.unwrap();
            APP_CONFIG.write().replace(app_config);
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
    rsx! {
        Router::<Route> {
            config: move || {
                RouterConfig::default()
                    .history(WebHistory::<Route>::default())
                    .on_update(move |_state| {
                        UI_MODEL.write().error_message = None;
                        UI_MODEL.write().confirmation_message = None;
                        None
                    })
            }
        }
    }
}

fn setup_key_bindings(
    mut ui_model: Signal<UniversalInboxUIModel>,
    notification_service_handle: Coroutine<NotificationCommand>,
    _task_service_handle: Coroutine<TaskCommand>,
    notifications_page: ReadOnlySignal<Page<NotificationWithTask>>,
) -> Option<()> {
    let window = web_sys::window()?;
    let document = window.document()?;
    let runtime = Runtime::current().unwrap();
    let scope_id = current_scope_id().unwrap();

    let handler = Closure::wrap(Box::new(move |evt: KeyboardEvent| {
        runtime.spawn(scope_id, async move {
            let notifications_page = notifications_page();
            let list_length = notifications_page.content.len();
            let selected_notification = notifications_page
                .content
                .get(ui_model.write().selected_notification_index);
            let current_url = current_location().unwrap();
            let mut handled = true;

            if current_url.path() != "/" {
                // The notification page
                handled = false;
            } else if ui_model.read().task_planning_modal_opened
                || ui_model.read().task_link_modal_opened
            {
                match evt.key().as_ref() {
                    "Escape" => {
                        ui_model.write().task_planning_modal_opened = false;
                        ui_model.write().task_link_modal_opened = false;
                    }
                    _ => handled = false,
                }
            } else {
                match evt.key().as_ref() {
                    "ArrowDown"
                        if ui_model.read().selected_notification_index < (list_length - 1) =>
                    {
                        let mut ui_model = ui_model.write();
                        let new_selected_notification_index =
                            ui_model.selected_notification_index + 1;
                        select_notification(&mut ui_model, new_selected_notification_index)
                            .unwrap_or_else(|_| {
                                panic!(
                                "Failed to select notification {new_selected_notification_index}"
                            )
                            });
                    }
                    "ArrowUp" if ui_model.read().selected_notification_index > 0 => {
                        let mut ui_model = ui_model.write();
                        let new_selected_notification_index =
                            ui_model.selected_notification_index - 1;
                        select_notification(&mut ui_model, new_selected_notification_index)
                            .unwrap_or_else(|_| {
                                panic!(
                                "Failed to select notification {new_selected_notification_index}"
                            )
                            });
                    }
                    "ArrowRight"
                        if ui_model.read().selected_preview_pane == PreviewPane::Notification
                            && selected_notification
                                .map(|notif| notif.task.is_some())
                                .unwrap_or_default() =>
                    {
                        ui_model.write().selected_preview_pane = PreviewPane::Task;
                    }
                    "ArrowLeft"
                        if ui_model.read().selected_preview_pane == PreviewPane::Task
                            && !selected_notification
                                .map(|notif| notif.is_built_from_task())
                                .unwrap_or_default() =>
                    {
                        ui_model.write().selected_preview_pane = PreviewPane::Notification;
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
                                NotificationCommand::CompleteTaskFromNotification(
                                    notification.clone(),
                                ),
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
                    "p" => ui_model.write().task_planning_modal_opened = true,
                    "l" => ui_model.write().task_link_modal_opened = true,
                    "Enter" => {
                        if let Some(notification) = selected_notification {
                            let _ = open_link(notification.get_html_url().as_str());
                        }
                    }
                    "h" | "?" => ui_model.write().toggle_help(),
                    _ => handled = false,
                }
            }

            if handled {
                ui_model.write().set_unhover_element(true);
                evt.prevent_default();
            }
        });
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
