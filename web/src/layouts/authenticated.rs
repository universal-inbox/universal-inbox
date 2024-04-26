#![allow(non_snake_case)]

use dioxus::prelude::*;

use crate::{
    auth::Authenticated,
    components::spinner::Spinner,
    config::{get_api_base_url, APP_CONFIG},
    model::UI_MODEL,
    route::Route,
    services::{
        integration_connection_service::{IntegrationConnectionCommand, INTEGRATION_CONNECTIONS},
        notification_service::NotificationCommand,
        user_service::UserCommand,
    },
};

#[component]
pub fn AuthenticatedLayout() -> Element {
    let api_base_url = use_memo(move || get_api_base_url().unwrap());

    if let Some(app_config) = APP_CONFIG.read().as_ref() {
        return rsx! {
            Authenticated {
                authentication_config: app_config.authentication_config.clone(),
                ui_model: UI_MODEL.signal(),
                api_base_url: api_base_url(),

                AuthenticatedApp {}
            }
        };
    }

    rsx! {
        div {
            class: "h-full flex justify-center items-center",

            Spinner {}
            "Loading Universal Inbox..."
        }
    }
}

#[component]
pub fn AuthenticatedApp() -> Element {
    let user_service = use_coroutine_handle::<UserCommand>();
    let integration_connection_service = use_coroutine_handle::<IntegrationConnectionCommand>();
    let notification_service = use_coroutine_handle::<NotificationCommand>();
    let history = WebHistory::<Route>::default();
    let nav = use_navigator();

    let _ = use_resource(move || {
        to_owned![user_service];
        to_owned![integration_connection_service];
        to_owned![notification_service];

        async move {
            user_service.send(UserCommand::GetUser);
            // Load integration connections status
            integration_connection_service.send(IntegrationConnectionCommand::Refresh);
            notification_service.send(NotificationCommand::Refresh);
            // Refresh notifications every minute
            gloo_timers::callback::Interval::new(60_000, move || {
                notification_service.send(NotificationCommand::Refresh);
                integration_connection_service.send(IntegrationConnectionCommand::Refresh);
            })
            .forget();
        }
    });

    if let Some(integration_connections) = INTEGRATION_CONNECTIONS.read().as_ref() {
        if integration_connections.is_empty() && history.current_route() != (Route::SettingsPage {})
        {
            nav.push(Route::SettingsPage {});
            needs_update();
            None
        } else {
            rsx! { Outlet::<Route> {} }
        }
    } else {
        rsx! {
            div {
                class: "h-full flex justify-center items-center",

                Spinner {}
                "Loading Universal Inbox..."
            }
        }
    }
}
