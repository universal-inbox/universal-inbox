use gloo_timers::future::TimeoutFuture;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::HtmlInputElement;

pub async fn focus_and_select_element(id: &str) -> Result<HtmlInputElement, JsValue> {
    let window = web_sys::window().ok_or_else(|| "Unable to load `window`".to_string())?;
    let document = window
        .document()
        .ok_or_else(|| "Unable to load `document`".to_string())?;
    let elt = document
        .get_element_by_id(id)
        .ok_or(format!("Element `{id}` not found"))?
        .dyn_into::<HtmlInputElement>()?;

    TimeoutFuture::new(100).await;

    elt.select();

    Ok(elt)
}
