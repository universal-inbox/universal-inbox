use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Local, Utc};
use gloo_timers::future::TimeoutFuture;
use gloo_utils::errors::JsError;
use url::Url;
use wasm_bindgen::JsCast;
use web_sys::{
    Element, HtmlElement, HtmlInputElement, MouseEvent, MouseEventInit, ScrollBehavior,
    ScrollToOptions,
};

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

pub fn open_link(url: &str) -> Result<()> {
    // Open the link in a *background* tab so focus stays on Universal Inbox.
    // `window.open(..., "_blank")` foregrounds the new tab and there is no flag
    // to keep it in the background — browsers only open a background tab on a
    // modifier-click. So we synthesize one on a throwaway anchor. Both Ctrl and
    // Meta are set: each platform honors only its own (Cmd on macOS, Ctrl on
    // Win/Linux) and ignores the other, avoiding platform sniffing.
    let window = web_sys::window().context("Unable to get the window object")?;
    let document = window
        .document()
        .context("Unable to get the document object")?;
    let body = document.body().context("Unable to get the document body")?;

    let anchor = document
        .create_element("a")
        .map_err(|err| JsError::try_from(err).unwrap())?;
    anchor
        .set_attribute("href", url)
        .map_err(|err| JsError::try_from(err).unwrap())?;
    anchor
        .set_attribute("target", "_blank")
        .map_err(|err| JsError::try_from(err).unwrap())?;
    anchor
        .set_attribute("rel", "noopener")
        .map_err(|err| JsError::try_from(err).unwrap())?;

    // Some browsers only navigate from a synthetic click when the node is in
    // the document, so attach it, dispatch, then detach.
    body.append_child(&anchor)
        .map_err(|err| JsError::try_from(err).unwrap())?;

    let event_init = MouseEventInit::new();
    event_init.set_bubbles(true);
    event_init.set_cancelable(true);
    event_init.set_ctrl_key(true);
    event_init.set_meta_key(true);
    event_init.set_view(Some(&window));
    let event = MouseEvent::new_with_mouse_event_init_dict("click", &event_init)
        .map_err(|err| JsError::try_from(err).unwrap())?;
    anchor
        .dispatch_event(&event)
        .map_err(|err| JsError::try_from(err).unwrap())?;

    body.remove_child(&anchor)
        .map_err(|err| JsError::try_from(err).unwrap())?;

    Ok(())
}

pub async fn copy_to_clipboard(text: &str) -> Result<()> {
    wasm_bindgen_futures::JsFuture::from(
        web_sys::window()
            .context("Unable to get the window object")?
            .navigator()
            .clipboard()
            .write_text(text),
    )
    .await
    .map_err(|err| JsError::try_from(err).unwrap())
    .context("Unable to copy text into the clipboard")?;

    Ok(())
}

pub fn scroll_element(id: &str, by: f64) -> Result<()> {
    let elt = get_element_by_id(id)?;
    let scroll_options = ScrollToOptions::new();
    scroll_options.set_behavior(ScrollBehavior::Smooth);
    scroll_options.set_top(by);
    elt.scroll_by_with_scroll_to_options(&scroll_options);
    Ok(())
}

pub fn reset_scroll_top(id: &str) -> Result<()> {
    let elt = get_element_by_id(id)?;
    elt.set_scroll_top(0);
    Ok(())
}

pub fn scroll_element_by_page(id: &str) -> Result<()> {
    let elt = get_element_by_id(id)?;
    scroll_element(id, elt.client_height().into())
}

pub async fn create_navigator_credentials(
    options: web_sys::CredentialCreationOptions,
) -> Result<web_sys::PublicKeyCredential> {
    wasm_bindgen_futures::JsFuture::from(
        web_sys::window()
            .context("Unable to get the window object")?
            .navigator()
            .credentials()
            .create_with_options(&options)
            .map_err(|err| JsError::try_from(err).unwrap())
            .context("Unable to create credentials")?,
    )
    .await
    .map(web_sys::PublicKeyCredential::from)
    .map_err(|err| JsError::try_from(err).unwrap())
    .context("Failed to create public key for Passkey authentication")
}

pub async fn get_navigator_credentials(
    options: web_sys::CredentialRequestOptions,
) -> Result<web_sys::PublicKeyCredential> {
    wasm_bindgen_futures::JsFuture::from(
        web_sys::window()
            .context("Unable to get the window object")?
            .navigator()
            .credentials()
            .get_with_options(&options)
            .map_err(|err| JsError::try_from(err).unwrap())
            .context("Unable to get credentials")?,
    )
    .await
    .map(web_sys::PublicKeyCredential::from)
    .map_err(|err| JsError::try_from(err).unwrap())
    .context("Failed to get public key for Passkey authentication")
}

pub fn get_screen_width() -> Result<usize> {
    let window = web_sys::window().context("Unable to load `window`")?;
    Ok(window
        .inner_width()
        .map_err(|err| JsError::try_from(err).unwrap())?
        .as_f64()
        .unwrap_or_default() as usize)
}

pub fn scroll_element_into_view_by_class(
    container_id: &str,
    child_class: &str,
    child_index: usize,
) -> Result<()> {
    let window = web_sys::window().context("Unable to load `window`")?;
    let document = window.document().context("Unable to load `document`")?;

    // Get the container element
    let container = document
        .get_element_by_id(container_id)
        .context(format!("Container element `{container_id}` not found"))?;

    // Get all elements with the specified class within the container
    let elements = container
        .query_selector_all(&format!(".{}", child_class))
        .map_err(|err| JsError::try_from(err).unwrap())?;

    if let Some(target_element) = elements.get(child_index as u32)
        && let Some(element) = target_element.dyn_ref::<Element>()
    {
        element.scroll_into_view_with_bool(true);
    }

    Ok(())
}

/// Replace `[label](url)` markdown links with just their label text. The
/// preview header takes a plain `String` for the title, so when a title
/// originates from Slack message text (which can contain markdown links)
/// we need to render only the visible label.
pub fn strip_markdown_links(text: &str) -> String {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"\[([^\]]+)\]\([^)]+\)").unwrap());
    re.replace_all(text, "$1").into_owned()
}

/// Absolute timestamp like `Sep 16, 2023, 9:00 PM` — used as the `title`
/// tooltip on the relative-time badge in `ThreadedMessage`.
pub fn format_absolute_time(dt: DateTime<Utc>) -> String {
    dt.format("%b %-d, %Y, %-I:%M %p").to_string()
}

/// Wall-clock time like `9:00 PM` — used by `ThreadedMessageFollowup` to show
/// the per-message time inline without repeating the date.
pub fn format_clock_time(dt: DateTime<Utc>) -> String {
    dt.format("%-I:%M %p").to_string()
}

pub fn format_elapsed_time(updated_at: DateTime<Utc>) -> String {
    let now = Local::now().with_timezone(&Utc);
    let duration = now.signed_duration_since(updated_at);

    let total_seconds = duration.num_seconds();

    if total_seconds < 1 {
        return "now".to_string();
    }

    if total_seconds < 60 {
        format!("{}s", total_seconds)
    } else if total_seconds < 3600 {
        let minutes = total_seconds / 60;
        format!("{}m", minutes)
    } else if total_seconds < 86400 {
        let hours = total_seconds / 3600;
        format!("{}h", hours)
    } else if total_seconds < 172800 {
        "Yesterday".to_string()
    } else if total_seconds < 604800 {
        let days = total_seconds / 86400;
        format!("{}d", days)
    } else if total_seconds < 2592000 {
        let weeks = total_seconds / 604800;
        format!("{}w", weeks)
    } else if total_seconds < 31536000 {
        let months = total_seconds / 2592000;
        format!("{}mo", months)
    } else {
        let years = total_seconds / 31536000;
        format!("{}y", years)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn test_format_elapsed_time() {
        let now = Utc::now();

        assert_eq!(format_elapsed_time(now), "now");
        assert_eq!(format_elapsed_time(now - Duration::seconds(30)), "30s");
        assert_eq!(format_elapsed_time(now - Duration::seconds(1)), "1s");
        assert_eq!(format_elapsed_time(now - Duration::minutes(5)), "5m");
        assert_eq!(format_elapsed_time(now - Duration::minutes(1)), "1m");
        assert_eq!(format_elapsed_time(now - Duration::hours(2)), "2h");
        assert_eq!(format_elapsed_time(now - Duration::hours(1)), "1h");
        assert_eq!(format_elapsed_time(now - Duration::days(3)), "3d");
        assert_eq!(format_elapsed_time(now - Duration::days(1)), "Yesterday");
    }
}
