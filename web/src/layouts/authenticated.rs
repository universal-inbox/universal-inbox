#![allow(non_snake_case)]

use chrono::Utc;
use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use log::debug;

use crate::{
    auth::Authenticated,
    components::loading::Loading,
    config::{get_api_base_url, APP_CONFIG},
    model::{AuthenticationState, UI_MODEL},
    route::Route,
    services::{
        integration_connection_service::{IntegrationConnectionCommand, INTEGRATION_CONNECTIONS},
        notification_service::NotificationCommand,
        task_service::TaskCommand,
    },
};

#[component]
pub fn AuthenticatedLayout() -> Element {
    let api_base_url = use_memo(move || get_api_base_url().unwrap());

    if let Some(app_config) = APP_CONFIG.read().as_ref() {
        return rsx! {
            Authenticated {
                authentication_configs: app_config.authentication_configs.clone(),
                api_base_url: api_base_url(),

                AuthenticatedApp {}
            }
        };
    }

    rsx! { Loading { label: "Loading Universal Inbox..." } }
}

#[component]
pub fn AuthenticatedApp() -> Element {
    debug!("AuthenticatedApp: rendering");
    let integration_connection_service = use_coroutine_handle::<IntegrationConnectionCommand>();
    let notification_service = use_coroutine_handle::<NotificationCommand>();
    let task_service = use_coroutine_handle::<TaskCommand>();
    let nav = use_navigator();

    use_future(move || async move {
        if UI_MODEL.read().authentication_state == AuthenticationState::Authenticated {
            // Load integration connections status
            integration_connection_service.send(IntegrationConnectionCommand::Refresh);
            notification_service.send(NotificationCommand::Refresh);
            task_service.send(TaskCommand::RefreshSyncedTasks);
            loop {
                TimeoutFuture::new(10_000).await;
                if (Utc::now().timestamp() % 60) < 10 {
                    // Refresh notifications and integration connections every minute
                    notification_service.send(NotificationCommand::Refresh);
                    task_service.send(TaskCommand::RefreshSyncedTasks);
                    TimeoutFuture::new(200).await;
                    integration_connection_service.send(IntegrationConnectionCommand::Refresh);
                } else if UI_MODEL.read().is_syncing_notifications
                    || UI_MODEL.read().is_syncing_tasks
                {
                    // Refresh integration connections every 10 seconds if any of them is syncing
                    integration_connection_service.send(IntegrationConnectionCommand::Refresh);
                }
            }
        }
    });

    if let Some(integration_connections) = INTEGRATION_CONNECTIONS.read().as_ref() {
        if integration_connections.is_empty() && history().current_route() != *"/settings" {
            nav.push(Route::SettingsPage {});
            needs_update();
            rsx! {}
        } else {
            rsx! { Outlet::<Route> {} }
        }
    } else {
        rsx! { Loading { label: "Loading Universal Inbox..." } }
    }
}
