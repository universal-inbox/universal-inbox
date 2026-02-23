use std::collections::HashMap;

use anyhow::{Result, anyhow};
use dioxus::prelude::*;
use log::{debug, error};
use reqwest::{
    Client, Method, Response, StatusCode,
    header::{HeaderMap, HeaderValue},
};
use serde_json;
use url::Url;

use crate::{
    components::toast_zone::{Toast, ToastKind},
    model::{AuthenticationState, UniversalInboxUIModel},
    services::{
        toast_service::{ToastCommand, ToastUpdate},
        version::check_version_mismatch,
    },
};

pub async fn call_api<R: for<'de> serde::de::Deserialize<'de>, B: serde::Serialize>(
    method: Method,
    base_url: &Url,
    path: &str,
    body: Option<B>,
    ui_model: Option<Signal<UniversalInboxUIModel>>,
) -> Result<R> {
    let mut request = with_credentials(API_CLIENT.request(method, base_url.join(path)?));

    if let Some(body) = body {
        request = request
            .header("content-type", "application/json")
            .json(&body);
    }

    let response: Response = request.send().await?;

    if let Some(backend_version) = response.headers().get("x-app-version")
        && let Ok(version_str) = backend_version.to_str()
    {
        check_version_mismatch(version_str);
    }

    if response.status().is_server_error() || response.status() == StatusCode::BAD_REQUEST {
        let default_error_message = "Error calling Universal Inbox API".to_string();
        if Some(HeaderValue::from_static("application/json"))
            == response.headers().get("content-type").cloned()
        {
            let message: HashMap<String, String> = response.json().await?;
            return Err(anyhow!(
                message
                    .get("message")
                    .cloned()
                    .unwrap_or(default_error_message)
            ));
        } else {
            error!(
                "Error calling Universal Inbox API: {:?}",
                response.text().await?
            );
            return Err(anyhow!(default_error_message));
        }
    }

    if let Some(mut ui_model) = ui_model {
        if response.status() == StatusCode::UNAUTHORIZED {
            if ui_model.read().authentication_state == AuthenticationState::Unknown
                || ui_model.read().authentication_state != AuthenticationState::Authenticated
                || ui_model.read().authentication_state != AuthenticationState::NotAuthenticated
            {
                ui_model.write().authentication_state = AuthenticationState::NotAuthenticated;
            }
            let default_error_message = "Unauthenticated call to the API".to_string();
            if Some(HeaderValue::from_static("application/json"))
                == response.headers().get("content-type").cloned()
            {
                let message: HashMap<String, String> = response.json().await?;
                return Err(anyhow!(
                    message
                        .get("message")
                        .cloned()
                        .unwrap_or(default_error_message)
                ));
            } else {
                return Err(anyhow!(default_error_message));
            }
        } else if ui_model.read().authentication_state != AuthenticationState::Authenticated {
            ui_model.write().authentication_state = AuthenticationState::Authenticated;
        }
    }

    // Handle 304 Not Modified responses as successful
    if response.status() == StatusCode::NOT_MODIFIED {
        debug!("Received 304 Not Modified response from {}", response.url());

        let empty_value_result = serde_json::from_str::<R>("{}")
            .or_else(|_| serde_json::from_str::<R>("[]"))
            .or_else(|_| serde_json::from_str::<R>("null"));
        if let Ok(empty_value) = empty_value_result {
            return Ok(empty_value);
        }

        debug!("All deserialization attempts of an empty result failed for 304 response");
        // Just continue with normal processing, which might fail
        // but at least we tried to handle 304 specially
    }

    Ok(response.json().await?)
}

#[allow(clippy::too_many_arguments)]
pub async fn call_api_and_notify<R: for<'de> serde::de::Deserialize<'de>, B: serde::Serialize>(
    method: Method,
    base_url: &Url,
    path: &str,
    body: Option<B>,
    ui_model: Option<Signal<UniversalInboxUIModel>>,
    toast_service: &Coroutine<ToastCommand>,
    loading_message: &str,
    success_message: &str,
) -> Result<R> {
    let toast = Toast {
        kind: ToastKind::Loading,
        message: loading_message.to_string(),
        ..Default::default()
    };
    let toast_id = toast.id;
    toast_service.send(ToastCommand::Push(toast));

    call_api(method.clone(), base_url, path, body, ui_model)
        .await
        .inspect(|_| {
            let toast_update = ToastUpdate {
                id: toast_id,
                kind: Some(ToastKind::Success),
                message: Some(success_message.to_string()),
                timeout: Some(Some(5_000)),
            };
            toast_service.send(ToastCommand::Update(toast_update));
        })
        .inspect_err(|error| {
            error!("An error occurred while calling the API ({method} {base_url}{path}): {error:?}");
            let toast_update = ToastUpdate {
                id: toast_id,
                kind: Some(ToastKind::Failure),
                message: Some("An error occurred while calling the Universal Inbox API. Please, retry 🙏 If the issue keeps happening, please contact our support.".to_string()),
                timeout: Some(Some(10_000)),
            };
            toast_service.send(ToastCommand::Update(toast_update));
        })
}

#[cfg(target_arch = "wasm32")]
lazy_static! {
    pub static ref API_CLIENT: Client = reqwest::ClientBuilder::new()
        .default_headers({
            let mut headers = HeaderMap::new();
            headers.insert("Accept", HeaderValue::from_static("application/json"));
            headers
        })
        .build()
        .unwrap();
}

#[cfg(not(target_arch = "wasm32"))]
lazy_static! {
    pub static ref API_CLIENT: Client = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers({
            let mut headers = HeaderMap::new();
            headers.insert("Accept", HeaderValue::from_static("application/json"));
            headers
        })
        .build()
        .unwrap();
}

#[cfg(target_arch = "wasm32")]
fn with_credentials(builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    builder.fetch_credentials_include()
}

#[cfg(not(target_arch = "wasm32"))]
fn with_credentials(builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    builder
}
