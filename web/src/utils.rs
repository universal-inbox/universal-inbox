use anyhow::{anyhow, Context, Result};
use gloo_timers::future::TimeoutFuture;
use gloo_utils::errors::JsError;
use url::Url;
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement, HtmlInputElement, Window};

pub async fn focus_and_select_input_element(id: &str) -> Result<HtmlInputElement> {
    let elt = wait_for_element_by_id(id, 300)
        .await?
        .dyn_into::<HtmlInputElement>()
        .map_err(|_| anyhow!("Unable to convert Element {id} into HtmlElement"))?;

    TimeoutFuture::new(100).await;

    elt.select();

    Ok(elt)
}

pub async fn focus_element(id: &str) -> Result<HtmlElement> {
    let elt = wait_for_element_by_id(id, 300)
        .await?
        .dyn_into::<HtmlElement>()
        .map_err(|_| anyhow!("Unable to convert Element {id} into HtmlElement"))?;

    TimeoutFuture::new(100).await;

    elt.focus().map_err(|err| JsError::try_from(err).unwrap())?;

    Ok(elt)
}

#[allow(dead_code)]
pub fn get_element_by_id(id: &str) -> Result<Element> {
    let window = web_sys::window().context("Unable to load `window`")?;
    let document = window.document().context("Unable to load `document`")?;
    document
        .get_element_by_id(id)
        .context(format!("Element `{id}` not found"))
}

pub async fn wait_for_element_by_id(id: &str, timeout: u32) -> Result<Element> {
    let max_loops = timeout / 10;
    let window = web_sys::window().context("Unable to load `window`")?;
    let document = window.document().context("Unable to load `document`")?;
    let mut loops = 0;
    while document.get_element_by_id(id).is_none() {
        TimeoutFuture::new(10).await;
        loops += 1;
        if loops >= max_loops {
            return Err(anyhow!("Element `{id}` not found"));
        }
    }
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

pub fn compute_text_color_from_background_color(color: &str) -> String {
    let color = color.trim_start_matches('#');
    let r = u8::from_str_radix(&color[0..2], 16).unwrap();
    let g = u8::from_str_radix(&color[2..4], 16).unwrap();
    let b = u8::from_str_radix(&color[4..6], 16).unwrap();

    let luminance = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) / 255.0;

    if luminance > 0.5 {
        "text-black".to_string()
    } else {
        "text-white".to_string()
    }
}

pub fn open_link(url: &str) -> Result<Window> {
    let window = web_sys::window().context("Unable to get the window object")?;
    window
        .open_with_url_and_target(url, "_blank")
        .map_err(|err| JsError::try_from(err).unwrap())?
        .context("Unable to open the link in a new tab")
}

pub async fn copy_to_clipboard(text: &str) -> Result<()> {
    wasm_bindgen_futures::JsFuture::from(
        web_sys::window()
            .context("Unable to get the window object")?
            .navigator()
            .clipboard()
            .context("Unable to get the clipboard object")?
            .write_text(text),
    )
    .await
    .map_err(|err| JsError::try_from(err).unwrap())
    .context("Unable to copy text into the clipboard")?;

    Ok(())
}
