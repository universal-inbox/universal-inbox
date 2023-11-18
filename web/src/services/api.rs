use anyhow::{anyhow, Result};
use dioxus::prelude::Coroutine;
use fermi::UseAtomRef;
use log::error;
use reqwest::{
    header::{HeaderMap, HeaderValue},
    Client, Method, Response, StatusCode,
};
use url::Url;

use crate::{
    components::toast_zone::{Toast, ToastKind},
    model::{AuthenticationState, UniversalInboxUIModel},
    services::toast_service::{ToastCommand, ToastUpdate},
};

pub async fn call_api<R: for<'de> serde::de::Deserialize<'de>, B: serde::Serialize>(
    method: Method,
    base_url: &Url,
    path: &str,
    body: Option<B>,
    ui_model_ref: Option<UseAtomRef<UniversalInboxUIModel>>,
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

    if let Some(ui_model_ref) = ui_model_ref {
        let mut ui_model_ref = ui_model_ref.write();
        if response.status() == StatusCode::UNAUTHORIZED {
            if ui_model_ref.authentication_state == AuthenticationState::Unknown
                || ui_model_ref.authentication_state != AuthenticationState::Authenticated
            {
                ui_model_ref.authentication_state = AuthenticationState::NotAuthenticated;
            }
            return Err(anyhow!("Unauthorized call to the API"));
        } else if ui_model_ref.authentication_state != AuthenticationState::Authenticated {
            ui_model_ref.authentication_state = AuthenticationState::Authenticated;
        }
    }

    Ok(response.json().await?)
}

#[allow(clippy::too_many_arguments)]
pub async fn call_api_and_notify<R: for<'de> serde::de::Deserialize<'de>, B: serde::Serialize>(
    method: Method,
    base_url: &Url,
    path: &str,
    body: Option<B>,
    ui_model_ref: Option<UseAtomRef<UniversalInboxUIModel>>,
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

    call_api(method.clone(), base_url, path, body, ui_model_ref)
        .await
        .map(|result: R| {
            let toast_update = ToastUpdate {
                id: toast_id,
                kind: Some(ToastKind::Success),
                message: Some(success_message.to_string()),
                timeout: Some(Some(5_000)),
            };
            toast_service.send(ToastCommand::Update(toast_update));
            result
        })
        .map_err(|error| {
            error!("An error occurred while calling the API ({method} {base_url}{path}): {error:?}");
            let toast_update = ToastUpdate {
                id: toast_id,
                kind: Some(ToastKind::Failure),
                message: Some("An error occurred while calling the Universal Inbox API. Please, retry 🙏 If the issue keeps happening, please contact our support.".to_string()),
                timeout: Some(Some(5_000)),
            };
            toast_service.send(ToastCommand::Update(toast_update));
            error
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
