use anyhow::{anyhow, Context, Result};
use gloo_timers::future::TimeoutFuture;
use gloo_utils::errors::JsError;
use url::Url;
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement, HtmlInputElement};

pub async fn focus_and_select_input_element(id: &str) -> Result<HtmlInputElement> {
    let elt = get_element_by_id(id)?
        .dyn_into::<HtmlInputElement>()
        .map_err(|_| anyhow!("Unable to convert Element {id} into HtmlElement"))?;

    TimeoutFuture::new(100).await;

    elt.select();

    Ok(elt)
}

pub async fn focus_element(id: &str) -> Result<HtmlElement> {
    let elt = get_element_by_id(id)?
        .dyn_into::<HtmlElement>()
        .map_err(|_| anyhow!("Unable to convert Element {id} into HtmlElement"))?;

    TimeoutFuture::new(100).await;

    elt.focus().map_err(|err| JsError::try_from(err).unwrap())?;

    Ok(elt)
}

fn get_element_by_id(id: &str) -> Result<Element> {
    let window = web_sys::window().context("Unable to load `window`")?;
    let document = window.document().context("Unable to load `document`")?;
    document
        .get_element_by_id(id)
        .context(format!("Element `{id}` not found"))
}

pub fn redirect_to(url: &str) -> Result<()> {
    let window = web_sys::window().context("Unable to load `window`")?;
    Ok(window
        .location()
        .assign(url)
        .map_err(|err| JsError::try_from(err).unwrap())?)
}

pub fn current_location() -> Result<Url> {
    let window = web_sys::window().context("Unable to load `window`")?;
    Ok(Url::parse(
        &window
            .location()
            .href()
            .map_err(|err| JsError::try_from(err).unwrap())?,
    )?)
}

pub fn current_origin() -> Result<Url> {
    let window = web_sys::window().context("Unable to load `window`")?;
    Ok(Url::parse(
        &window
            .location()
            .origin()
            .map_err(|err| JsError::try_from(err).unwrap())?,
    )?)
}

pub fn get_local_storage() -> Result<web_sys::Storage> {
    let window = web_sys::window().context("Unable to get the window object")?;
    window
        .local_storage()
        .map_err(|err| JsError::try_from(err).unwrap())?
        .context("No local storage available")
}
