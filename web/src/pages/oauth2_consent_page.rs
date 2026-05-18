#![allow(non_snake_case)]

use dioxus::prelude::*;
use log::error;

use universal_inbox::auth::oauth2::{OAuth2ConsentDecision, OAuth2ConsentRequest};

use crate::{
    components::{
        loading::Loading,
        ui::{Button, ButtonSize, ButtonVariant, PageHeader},
    },
    config::get_api_base_url,
    services::oauth2_consent_service::{fetch_consent_request, submit_consent_decision},
    utils::redirect_to,
};

#[component]
pub fn OAuth2ConsentPage(request_id: String) -> Element {
    let api_base_url = use_memo(move || get_api_base_url().unwrap());
    let request_id_signal = use_signal(|| request_id.clone());
    let submitting = use_signal(|| false);
    let error_message = use_signal(|| None::<String>);

    let consent_request = use_resource(move || {
        let request_id = request_id_signal();
        let api_base_url = api_base_url();
        async move {
            fetch_consent_request(&api_base_url, &request_id)
                .await
                .map_err(|err| err.to_string())
        }
    });

    let consent_request_value = consent_request.read();
    let Some(consent_request_result) = consent_request_value.as_ref() else {
        return rsx! { Loading { label: "Loading consent request..." } };
    };

    let consent: OAuth2ConsentRequest = match consent_request_result {
        Ok(req) => req.clone(),
        Err(message) => {
            return rsx! {
                PageHeader { title: "Consent request unavailable".to_string() }
                p { class: "text-sm text-ui-base-muted leading-normal mb-7",
                    "{message}"
                }
            };
        }
    };

    let scope_label = consent
        .scope
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "default".to_string());
    let client_label = consent
        .client_name
        .clone()
        .unwrap_or_else(|| consent.client_id.clone());

    let consent_request_id = consent.request_id.clone();
    let consent_csrf_token = consent.csrf_token.clone();

    let on_deny_request_id = consent_request_id.clone();
    let on_deny_csrf_token = consent_csrf_token.clone();
    let on_allow_request_id = consent_request_id;
    let on_allow_csrf_token = consent_csrf_token;

    rsx! {
        PageHeader {
            title: "Authorize access".to_string(),
            subtitle: Some(format!("{client_label} wants to access your Universal Inbox.")),
        }
        div { class: "rounded-ui-md border border-ui-border bg-ui-surface p-4 mb-6",
            p { class: "text-xs uppercase tracking-wide text-ui-base-muted font-semibold mb-1",
                "Requested permissions"
            }
            p { class: "text-sm text-ui-base-content font-mono break-all", "{scope_label}" }
            p { class: "text-xs text-ui-base-muted mt-3",
                "Redirect URI: "
                span { class: "font-mono break-all", "{consent.redirect_uri}" }
            }
        }
        if let Some(msg) = error_message.read().as_ref() {
            div { class: "rounded-ui-md border border-ui-error bg-ui-surface text-ui-error text-sm p-3 mb-4",
                "{msg}"
            }
        }
        div { class: "grid grid-cols-2 gap-2.5",
            Button {
                variant: ButtonVariant::Ghost,
                size: ButtonSize::Md,
                class: "btn-block".to_string(),
                disabled: submitting(),
                onclick: move |_| {
                    let api_base_url = api_base_url();
                    let request_id = on_deny_request_id.clone();
                    let csrf_token = on_deny_csrf_token.clone();
                    spawn(async move {
                        submit_decision(
                            api_base_url,
                            request_id,
                            csrf_token,
                            OAuth2ConsentDecision::Deny,
                            submitting,
                            error_message,
                        )
                        .await;
                    });
                },
                "Deny"
            }
            Button {
                variant: ButtonVariant::Primary,
                size: ButtonSize::Md,
                class: "btn-block".to_string(),
                disabled: submitting(),
                onclick: move |_| {
                    let api_base_url = api_base_url();
                    let request_id = on_allow_request_id.clone();
                    let csrf_token = on_allow_csrf_token.clone();
                    spawn(async move {
                        submit_decision(
                            api_base_url,
                            request_id,
                            csrf_token,
                            OAuth2ConsentDecision::Allow,
                            submitting,
                            error_message,
                        )
                        .await;
                    });
                },
                "Allow"
            }
        }
    }
}

async fn submit_decision(
    api_base_url: url::Url,
    request_id: String,
    csrf_token: String,
    decision: OAuth2ConsentDecision,
    mut submitting: Signal<bool>,
    mut error_message: Signal<Option<String>>,
) {
    *submitting.write() = true;
    *error_message.write() = None;
    match submit_consent_decision(&api_base_url, &request_id, &csrf_token, decision).await {
        Ok(response) => {
            if let Err(err) = redirect_to(&response.redirect_url) {
                error!("Failed to redirect after consent decision: {err:?}");
                *error_message.write() =
                    Some("Failed to navigate back to the application".to_string());
                *submitting.write() = false;
            }
        }
        Err(err) => {
            error!("Failed to submit OAuth2 consent decision: {err:?}");
            *error_message.write() = Some(err.to_string());
            *submitting.write() = false;
        }
    }
}
