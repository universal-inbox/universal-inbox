#![allow(non_snake_case)]

#[macro_use]
extern crate lazy_static;

use cfg_if::cfg_if;
use dioxus::prelude::dioxus_core::Runtime;
use dioxus::prelude::dioxus_router::RouterConfig;
use dioxus::prelude::*;
#[cfg(feature = "web")]
use gloo_utils::errors::JsError;
#[cfg(feature = "web")]
use keyboard_manager::{KeyboardHandler, KeyboardManager};
use log::debug;
#[cfg(feature = "web")]
use wasm_bindgen::{JsCast, prelude::Closure};
#[cfg(feature = "web")]
use web_sys::KeyboardEvent;

use config::{APP_CONFIG, get_api_base_url, get_app_config};
use model::UI_MODEL;
use route::Route;
use services::{
    authentication_token_service::{
        AUTHENTICATION_TOKENS, CREATED_AUTHENTICATION_TOKEN, authentication_token_service,
    },
    integration_connection_service::{INTEGRATION_CONNECTIONS, integration_connnection_service},
    notification_service::{NOTIFICATION_FILTERS, NOTIFICATIONS_PAGE, notification_service},
    task_service::task_service,
    toast_service::{TOASTS, VIEWPORT_WIDTH, toast_service},
    user_service::{CONNECTED_USER, user_service},
};
use theme::{IS_DARK_MODE, toggle_dark_mode};
#[cfg(feature = "web")]
use utils::{current_location, get_local_storage};

#[cfg(feature = "web")]
use crate::keyboard_manager::KEYBOARD_MANAGER;
use crate::services::{
    integration_connection_service::TASK_SERVICE_INTEGRATION_CONNECTION,
    task_service::SYNCED_TASKS_PAGE,
};

mod auth;
mod components;
mod config;
mod form;
mod icons;
mod images;
#[cfg(feature = "web")]
mod keyboard_manager;
mod layouts;
mod model;
mod pages;
mod route;
mod services;
mod settings;
mod theme;
mod utils;

pub fn App() -> Element {
    let api_base_url = use_memo(move || get_api_base_url().unwrap());

    let toast_service_handle = use_coroutine(|rx| toast_service(rx, TOASTS.signal()));
    let task_service_handle = use_coroutine(move |rx| {
        to_owned![toast_service_handle];

        task_service(
            rx,
            api_base_url(),
            SYNCED_TASKS_PAGE.signal(),
            UI_MODEL.signal(),
            toast_service_handle,
        )
    });
    let notification_service_handle = use_coroutine(move |rx| {
        to_owned![toast_service_handle];
        to_owned![task_service_handle];

        notification_service(
            rx,
            api_base_url(),
            NOTIFICATIONS_PAGE.signal(),
            NOTIFICATION_FILTERS.signal(),
            UI_MODEL.signal(),
            task_service_handle,
            toast_service_handle,
        )
    });
    let _user_service_handle = use_coroutine(move |rx| {
        user_service(
            rx,
            api_base_url(),
            CONNECTED_USER.signal(),
            UI_MODEL.signal(),
        )
    });
    let _integration_connection_service_handle = use_coroutine(move |rx| {
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

    let _authentication_token_service_handle = use_coroutine(move |rx| {
        authentication_token_service(
            rx,
            api_base_url(),
            AUTHENTICATION_TOKENS.signal(),
            CREATED_AUTHENTICATION_TOKEN.signal(),
            UI_MODEL.signal(),
            toast_service_handle,
        )
    });

    // Initialize viewport width and set up resize listener
    use_effect(move || {
        #[cfg(feature = "web")]
        {
            // Set initial viewport width
            if let Some(window) = web_sys::window() {
                *VIEWPORT_WIDTH.write() = window
                    .inner_width()
                    .ok()
                    .and_then(|w| w.as_f64())
                    .unwrap_or(1024.0);

                // Set up resize listener
                let closure = Closure::wrap(Box::new(move || {
                    if let Some(window) = web_sys::window()
                        && let Some(width) = window.inner_width().ok().and_then(|w| w.as_f64())
                    {
                        *VIEWPORT_WIDTH.write() = width;
                    }
                }) as Box<dyn FnMut()>);

                if let Err(e) = window
                    .add_event_listener_with_callback("resize", closure.as_ref().unchecked_ref())
                {
                    debug!("Failed to add resize event listener: {:?}", e);
                }

                // Keep closure alive
                closure.forget();
            }
        }
    });

    use_future(move || async move {
        *IS_DARK_MODE.write() = toggle_dark_mode(false).expect("Failed to initialize the theme");

        #[cfg(feature = "web")]
        setup_key_bindings(KEYBOARD_MANAGER.signal().into());

        match get_app_config().await {
            Ok(app_config) => {
                APP_CONFIG.write().replace(app_config);
            }
            Err(err) => {
                debug!("Failed to load app config: {err:?}");
            }
        }
    });

    // Dioxus 0.4.1 bug workaround: https://github.com/DioxusLabs/dioxus/issues/1511
    #[cfg(feature = "web")]
    {
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
    }
    // end workaround

    debug!("Rendering app");
    cfg_if! {
        if #[cfg(feature = "trunk")] {
            let head = rsx! {};
        } else {
            let head = rsx! {
                document::Stylesheet { href: asset!("/public/css/flatpickr.min.css") }
                document::Stylesheet { href: asset!("/public/css/universal-inbox.min.css") }
                document::Link { rel: "icon", href: asset!("/images/favicon.ico") }
            };
        }
    }

    rsx! {
        { head }

        Router::<Route> {
            config: move || {
                RouterConfig::default()
            }
        }
    }
}

#[cfg(feature = "web")]
fn setup_key_bindings(keyboard_manager: ReadSignal<KeyboardManager>) -> Option<()> {
    let window = web_sys::window()?;
    let document = window.document()?;
    let runtime = Runtime::current();
    let scope_id = runtime.current_scope_id();

    let handler = Closure::wrap(Box::new(move |evt: KeyboardEvent| {
        runtime.spawn(scope_id, async move {
            if keyboard_manager.read().handle_keydown(&evt) {
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
