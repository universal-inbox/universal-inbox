#![allow(non_snake_case)]

use dioxus::prelude::*;

use log::{debug, warn};

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, config::IntegrationConnectionConfig,
        provider::IntegrationProviderKind,
    },
    user::UserPreferencesPatch,
};

use crate::{
    components::{
        integrations_panel::IntegrationsPanel,
        loading::Loading,
        settings_controls::SettingRow,
        toast_zone::{Toast, ToastKind},
        ui::{Card, CardVariant, Overline, ToggleSize, ToggleSwitch},
    },
    config::APP_CONFIG,
    model::UI_MODEL,
    services::{
        integration_connection_service::{INTEGRATION_CONNECTIONS, IntegrationConnectionCommand},
        toast_service::ToastCommand,
        user_preferences_service::{USER_PREFERENCES, UserPreferencesCommand},
    },
    utils::current_location,
};

/// Map the kebab-case OAuth callback reason codes emitted by the API
/// (`api/src/routes/oauth.rs::OAuthCallbackErrorCode`) to user-facing
/// messages. Keep these terse and consistent with existing failure toasts in
/// `integration_connection_service.rs`. Unknown codes fall back to a generic
/// message so a future server-side code never renders raw to the user.
fn oauth_error_message(code: &str) -> &'static str {
    match code {
        "invalid-state" => {
            "The OAuth response was invalid. Please retry the connection from the start."
        }
        "expired-state" => "The OAuth request expired. Please retry the connection from the start.",
        "provider-error" => {
            "The integration provider rejected the connection. Please retry, and if it keeps happening contact our support."
        }
        // "internal-error" plus any future/unknown code.
        _ => {
            "An error occurred while completing the OAuth flow. Please retry 🙏 If the issue keeps happening, please contact our support."
        }
    }
}

/// Strip `oauth_error` / `oauth_success` from the current URL after we've
/// consumed them so that a page reload does not re-trigger the toast.
fn clear_oauth_query_params() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(history) = window.history() else {
        return;
    };
    let Ok(pathname) = window.location().pathname() else {
        return;
    };
    // `replaceState(null, "", "/settings")` rewrites the URL bar without
    // triggering a navigation event, dropping the query string entirely.
    let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&pathname));
}

pub fn SettingsPage() -> Element {
    let integration_connection_service = use_coroutine_handle::<IntegrationConnectionCommand>();
    let user_preferences_service = use_coroutine_handle::<UserPreferencesCommand>();
    let toast_service = use_coroutine_handle::<ToastCommand>();

    debug!("Rendering settings page");

    let _resource = use_resource(move || {
        to_owned![integration_connection_service, user_preferences_service];

        async move {
            integration_connection_service.send(IntegrationConnectionCommand::Refresh);
            user_preferences_service.send(UserPreferencesCommand::Refresh);
        }
    });

    // Surface a toast when the OAuth callback redirected back here with a
    // result. `current_location()` reads `window.location.href` and the API
    // only ever attaches one of `oauth_error` (sanitized code) or
    // `oauth_success=true`.
    use_hook(move || {
        if let Ok(url) = current_location() {
            let mut error_code: Option<String> = None;
            let mut success = false;
            for (k, v) in url.query_pairs() {
                if k == "oauth_error" {
                    error_code = Some(v.to_string());
                } else if k == "oauth_success" && v == "true" {
                    success = true;
                }
            }
            if let Some(code) = error_code {
                warn!("Surfacing OAuth callback error: {code}");
                toast_service.send(ToastCommand::Push(Toast {
                    kind: ToastKind::Failure,
                    message: oauth_error_message(&code).to_string(),
                    timeout: Some(10_000),
                    ..Default::default()
                }));
                clear_oauth_query_params();
            } else if success {
                toast_service.send(ToastCommand::Push(Toast {
                    kind: ToastKind::Success,
                    message: "Integration connected successfully.".to_string(),
                    timeout: Some(5_000),
                    ..Default::default()
                }));
                clear_oauth_query_params();
            }
        }
    });

    let open_links_in_background = USER_PREFERENCES
        .read()
        .as_ref()
        .map(|prefs| prefs.open_links_in_background)
        .unwrap_or(false);

    if let Some(app_config) = APP_CONFIG.read().as_ref()
        && let Some(integration_connections) = INTEGRATION_CONNECTIONS.read().as_ref()
    {
        return rsx! {
            div {
                class: "flex-1 overflow-y-auto bg-ui-base-200",

                div {
                    // Settings shell: 16/20/40 padding, 740px max width, fade-up
                    // on mount. The `animate-detail-fade` utility maps to the
                    // `detail-fade` keyframe via the `--animate-detail-fade`
                    // @theme entry; same animation timing as the legacy
                    // `.settings-container` rule.
                    class: "px-5 pt-4 pb-10 max-w-[740px] mx-auto animate-detail-fade",

                    IntegrationsPanel {
                        ui_model: UI_MODEL.signal(),
                        integration_providers: app_config.integration_providers.clone(),
                        integration_connections: integration_connections.clone(),
                        on_connect: move |(provider_kind, connection): (IntegrationProviderKind, Option<IntegrationConnection>)| {
                            if let Some(connection) = connection {
                                integration_connection_service.send(
                                    IntegrationConnectionCommand::AuthenticateIntegrationConnection(connection.clone())
                                );
                            } else {
                                integration_connection_service.send(
                                    IntegrationConnectionCommand::CreateIntegrationConnection(provider_kind)
                                );
                            }
                        },
                        on_disconnect: move |connection: IntegrationConnection| {
                            integration_connection_service.send(
                                IntegrationConnectionCommand::DisconnectIntegrationConnection(connection.id)
                            );
                        },
                        on_reconnect: move |connection: IntegrationConnection| {
                            integration_connection_service.send(
                                IntegrationConnectionCommand::ReconnectIntegrationConnection(connection.clone())
                            );
                        },
                        on_config_change: move |(connection, config): (IntegrationConnection, IntegrationConnectionConfig)| {
                            integration_connection_service.send(
                                IntegrationConnectionCommand::UpdateIntegrationConnectionConfig(connection.clone(), config)
                            );
                        },
                    }

                    Overline { class: "mt-4".to_string(), "Preferences" }

                    Card {
                        variant: CardVariant::Default,
                        SettingRow {
                            label: rsx! { "Open links in a background tab" },
                            description: Some(
                                "When pressing Enter to open a notification or task source, \
                                 keep focus on Universal Inbox instead of switching to the new tab."
                                    .to_string(),
                            ),
                            ToggleSwitch {
                                size: ToggleSize::Md,
                                checked: open_links_in_background,
                                label: Some("Open links in a background tab".to_string()),
                                onchange: move |new_value: bool| {
                                    user_preferences_service.send(UserPreferencesCommand::Patch(
                                        UserPreferencesPatch {
                                            open_links_in_background: Some(new_value),
                                            ..Default::default()
                                        },
                                    ));
                                },
                            }
                        }
                    }
                }
            }
        };
    }

    rsx! { Loading { label: "Loading Universal Inbox settings..." } }
}
