use gloo_timers::future::TimeoutFuture;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Element, HtmlElement, HtmlInputElement};

pub async fn focus_and_select_input_element(id: &str) -> Result<HtmlInputElement, JsValue> {
    let elt = get_element_by_id(id)?.dyn_into::<HtmlInputElement>()?;

    TimeoutFuture::new(100).await;

    elt.select();

    Ok(elt)
}

pub async fn focus_element(id: &str) -> Result<HtmlElement, JsValue> {
    let elt = get_element_by_id(id)?.dyn_into::<HtmlElement>()?;

    TimeoutFuture::new(100).await;

    elt.focus()?;

    Ok(elt)
}

fn get_element_by_id(id: &str) -> Result<Element, JsValue> {
    let window = web_sys::window().ok_or_else(|| "Unable to load `window`".to_string())?;
    let document = window
        .document()
        .ok_or_else(|| "Unable to load `document`".to_string())?;
    Ok(document
        .get_element_by_id(id)
        .ok_or(format!("Element `{id}` not found"))?)
}
