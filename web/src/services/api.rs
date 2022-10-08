use std::collections::HashMap;

use universal_inbox::Notification;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response};

#[wasm_bindgen(module = "/js/api.js")]
extern "C" {
    fn get_api_base_url() -> String;
}

pub async fn call_api(
    method: &str,
    path: &str,
    headers: HashMap<String, String>,
) -> Result<Vec<Notification>, JsValue> {
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
