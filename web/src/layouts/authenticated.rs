#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_router::prelude::*;
use fermi::use_atom_ref;

use crate::{
    auth::Authenticated,
    components::spinner::Spinner,
    config::{get_api_base_url, APP_CONFIG},
    model::UI_MODEL,
    route::Route,
    services::{
        integration_connection_service::{IntegrationConnectionCommand, INTEGRATION_CONNECTIONS},
        notification_service::NotificationCommand,
    },
};

#[inline_props]
pub fn AuthenticatedLayout(cx: Scope) -> Element {
    let ui_model_ref = use_atom_ref(cx, &UI_MODEL);
    let app_config_ref = use_atom_ref(cx, &APP_CONFIG);
    let api_base_url = use_memo(cx, (), |()| get_api_base_url().unwrap());

    if let Some(app_config) = app_config_ref.read().as_ref() {
        return render! {
            Authenticated {
                issuer_url: app_config.oidc_issuer_url.clone(),
                client_id: app_config.oidc_client_id.clone(),
                redirect_url: app_config.oidc_redirect_url.clone(),
                ui_model_ref: ui_model_ref.clone(),
                api_base_url: api_base_url.clone(),

                AuthenticatedApp {}
            }
        };
    }

    render! {
        div {
            class: "h-full flex justify-center items-center",

            Spinner {}
            "Loading Universal Inbox..."
        }
    }
}

#[inline_props]
pub fn AuthenticatedApp(cx: Scope) -> Element {
    let integration_connections_ref = use_atom_ref(cx, &INTEGRATION_CONNECTIONS);
    let integration_connection_service =
        use_coroutine_handle::<IntegrationConnectionCommand>(cx).unwrap();
    let notification_service = use_coroutine_handle::<NotificationCommand>(cx).unwrap();
    let history = WebHistory::<Route>::default();
    let nav = use_navigator(cx);

    use_future(cx, (), |()| {
        to_owned![integration_connection_service];
        to_owned![notification_service];

        async move {
            // Load integration connections status
            integration_connection_service.send(IntegrationConnectionCommand::Refresh);

            notification_service.send(NotificationCommand::Refresh);
            // Refresh notifications every minute
            gloo_timers::callback::Interval::new(60_000, move || {
                notification_service.send(NotificationCommand::Refresh);
            })
            .forget();
        }
    });

    if let Some(integration_connections) = integration_connections_ref.read().as_ref() {
        if integration_connections.is_empty() && history.current_route() != (Route::SettingsPage {})
        {
            nav.push(Route::SettingsPage {});
            cx.needs_update();
            None
        } else {
            render! { Outlet::<Route> {} }
        }
    } else {
        render! {
            div {
                class: "h-full flex justify-center items-center",

                Spinner {}
                "Loading Universal Inbox..."
            }
        }
    }
}
