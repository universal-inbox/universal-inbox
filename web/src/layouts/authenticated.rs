#![allow(non_snake_case)]

use chrono::Utc;
use dioxus::prelude::{dioxus_core::use_drop, *};
use gloo_timers::future::TimeoutFuture;
use log::debug;

use crate::{
    auth::Authenticated,
    components::loading::Loading,
    config::{APP_CONFIG, get_api_base_url},
    model::{AuthenticationState, UI_MODEL},
    route::Route,
    services::{
        integration_connection_service::{INTEGRATION_CONNECTIONS, IntegrationConnectionCommand},
        notification_service::NotificationCommand,
        realtime_service::{RealtimeCommand, is_page_hidden},
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
    let realtime_service = use_coroutine_handle::<RealtimeCommand>();

    use_drop(move || realtime_service.send(RealtimeCommand::Disconnect));

    use_future(move || async move {
        if UI_MODEL.read().authentication_state == AuthenticationState::Authenticated {
            // Load integration connections status
            integration_connection_service.send(IntegrationConnectionCommand::Refresh);
            notification_service.send(NotificationCommand::Refresh);
            task_service.send(TaskCommand::RefreshSyncedTasks);
            realtime_service.send(RealtimeCommand::Connect);
            loop {
                TimeoutFuture::new(10_000).await;
                // A hidden tab has nothing to show: once visible again, its event stream
                // reopens and refetches the lists
                if is_page_hidden() {
                    continue;
                }
                let is_realtime_connected = UI_MODEL.read().is_realtime_connected;
                // Pushed events refetch the changed lists: polling is then only a safety net
                let refresh_period_in_seconds = if is_realtime_connected { 300 } else { 60 };
                if (Utc::now().timestamp() % refresh_period_in_seconds) < 10 {
                    // Refresh notifications and integration connections every period
                    notification_service.send(NotificationCommand::Refresh);
                    task_service.send(TaskCommand::RefreshSyncedTasks);
                    TimeoutFuture::new(200).await;
                    integration_connection_service.send(IntegrationConnectionCommand::Refresh);
                } else if !is_realtime_connected
                    && (UI_MODEL.read().is_syncing_notifications
                        || UI_MODEL.read().is_syncing_tasks)
                {
                    // Refresh integration connections every 10 seconds if any of them is syncing
                    integration_connection_service.send(IntegrationConnectionCommand::Refresh);
                }
            }
        }
    });

    if INTEGRATION_CONNECTIONS.read().as_ref().is_some() {
        rsx! { Outlet::<Route> {} }
    } else {
        rsx! { Loading { label: "Loading Universal Inbox..." } }
    }
}
