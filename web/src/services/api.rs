use std::collections::HashMap;

use dioxus::prelude::CoroutineHandle;
use js_sys::JSON::stringify;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response};

use crate::components::toast_zone::Toast;
use crate::components::toast_zone::ToastKind;

use super::toast_service::ToastCommand;
use super::toast_service::ToastUpdate;

#[wasm_bindgen(module = "/js/api.js")]
extern "C" {
    fn get_api_base_url() -> String;
}

pub async fn call_api<R: for<'de> serde::de::Deserialize<'de>>(
    method: &str,
    path: &str,
    headers: HashMap<String, String>,
) -> Result<R, JsValue> {
    let mut opts = RequestInit::new();
    opts.method(method);
    opts.mode(RequestMode::Cors);

    let url = format!("{}{}", get_api_base_url(), path);
    let request = Request::new_with_str_and_init(&url, &opts)?;

    for (name, value) in headers {
        request.headers().set(&name, &value)?;
    }
    request.headers().set("Accept", "application/json")?;

    let window = web_sys::window().unwrap();
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;

    let resp: Response = resp_value.dyn_into().unwrap();

    // Convert this other `Promise` into a rust `Future`.
    let json = JsFuture::from(resp.json()?).await?;

    // +200KB vs JsValue
    Ok(serde_wasm_bindgen::from_value(json).unwrap())
}

pub async fn call_api_with_body<R: for<'de> serde::de::Deserialize<'de>, B: serde::Serialize>(
    method: &str,
    path: &str,
    body: B,
    headers: HashMap<String, String>,
) -> Result<R, JsValue> {
    let mut opts = RequestInit::new();
    opts.method(method);
    opts.mode(RequestMode::Cors);
    let body_value = stringify(&serde_wasm_bindgen::to_value(&body)?)?;
    opts.body(Some(&body_value));

    let url = format!("{}{}", get_api_base_url(), path);
    let request = Request::new_with_str_and_init(&url, &opts)?;

    for (name, value) in headers {
        request.headers().set(&name, &value)?;
    }
    request.headers().set("Accept", "application/json")?;
    request.headers().set("content-type", "application/json")?;

    let window = web_sys::window().unwrap();
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;

    let resp: Response = resp_value.dyn_into().unwrap();

    // Convert this other `Promise` into a rust `Future`.
    let json = JsFuture::from(resp.json()?).await?;

    // +200KB vs JsValue
    Ok(serde_wasm_bindgen::from_value(json).unwrap())
}

pub async fn call_api_and_notify<R: for<'de> serde::de::Deserialize<'de>, B: serde::Serialize>(
    method: &str,
    path: &str,
    body: B,
    headers: HashMap<String, String>,
    toast_service: &CoroutineHandle<ToastCommand>,
    loading_message: &str,
    success_message: &str,
) -> Result<R, JsValue> {
    let toast = Toast {
        kind: ToastKind::Loading,
        message: loading_message.to_string(),
        ..Default::default()
    };
    let toast_id = toast.id;
    toast_service.send(ToastCommand::Push(toast));

    let result: R = call_api_with_body(method, path, body, headers).await?;

    let toast_update = ToastUpdate {
        id: toast_id,
        kind: Some(ToastKind::Success),
        message: Some(success_message.to_string()),
        timeout: Some(Some(5_000)),
    };
    toast_service.send(ToastCommand::Update(toast_update));

    Ok(result)
}
