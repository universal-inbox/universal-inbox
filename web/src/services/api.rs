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
    let mut request = API_CLIENT
        .request(method, base_url.join(path)?)
        .fetch_credentials_include();

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

    let status = response.status();

    // Reflect a lost session in the UI model before surfacing the error.
    if status == StatusCode::UNAUTHORIZED
        && let Some(mut ui_model) = ui_model
    {
        ui_model.write().authentication_state = AuthenticationState::NotAuthenticated;
    }

    // Treat every non-success status (except 304, handled below) as a failure
    // and surface the API's `{"message": ...}` body. Previously only 5xx / 400 /
    // 401 were handled and any other error status — e.g. 429 Too Many Requests
    // from the login throttle — fell through to `response.json::<R>()`, which
    // tried to decode the JSON error body into the success type and failed with
    // a confusing "error decoding response body" instead of the real reason.
    if !status.is_success() && status != StatusCode::NOT_MODIFIED {
        let default_error_message = if status == StatusCode::UNAUTHORIZED {
            "Unauthenticated call to the API"
        } else {
            "Error calling Universal Inbox API"
        };
        return Err(extract_api_error(response, default_error_message).await);
    }

    // Successful call: mark the session authenticated.
    if let Some(mut ui_model) = ui_model
        && ui_model.read().authentication_state != AuthenticationState::Authenticated
    {
        ui_model.write().authentication_state = AuthenticationState::Authenticated;
    }

    // Handle 304 Not Modified responses as successful
    if status == StatusCode::NOT_MODIFIED {
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

/// Turn a non-success response into a user-facing error, preferring the API's
/// `{"message": ...}` body when present and falling back to `default_message`.
/// Consumes the response. Never tries to decode the body into the success type,
/// so a JSON error envelope can never surface as "error decoding response body".
async fn extract_api_error(response: Response, default_message: &str) -> anyhow::Error {
    let is_json = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        // Tolerate parameters such as `application/json; charset=utf-8`.
        .map(|content_type| content_type.starts_with("application/json"))
        .unwrap_or(false);

    if is_json {
        return match response.json::<HashMap<String, String>>().await {
            Ok(body) => anyhow!(
                body.get("message")
                    .cloned()
                    .unwrap_or_else(|| default_message.to_string())
            ),
            Err(decode_error) => {
                error!("Failed to decode Universal Inbox API error body: {decode_error:?}");
                anyhow!(default_message.to_string())
            }
        };
    }

    match response.text().await {
        Ok(text) => error!("Error calling Universal Inbox API: {text:?}"),
        Err(read_error) => error!("Failed to read Universal Inbox API error body: {read_error:?}"),
    }
    anyhow!(default_message.to_string())
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
