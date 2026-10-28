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
    // Foreground open (the default): `window.open(url, "_blank")` opens — and
    // focuses — a new tab. Users who prefer to keep focus on Universal Inbox
    // can enable the "open links in background" preference, which routes
    // through [`open_link_in_background`] instead.
    let window = web_sys::window().context("Unable to get the window object")?;
    window
        .open_with_url_and_target(url, "_blank")
        .map_err(|err| JsError::try_from(err).unwrap())?
        .context("Unable to open the link in a new tab")?;
    Ok(())
}

pub fn open_link_in_background(url: &str) -> Result<()> {
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

/// Scroll the `container_id` element so that `child_id` sits `offset` pixels
/// below its top edge. Only the container scrolls (unlike `scrollIntoView`,
/// which also scrolls every scrollable ancestor).
pub fn scroll_child_to_top(container_id: &str, child_id: &str, offset: f64) -> Result<()> {
    let container = get_element_by_id(container_id)?;
    let child = get_element_by_id(child_id)?;
    let delta = child.get_bounding_client_rect().top() - container.get_bounding_client_rect().top();
    container.set_scroll_top((container.scroll_top() as f64 + delta - offset).max(0.0) as i32);
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

/// Plain text of a markdown title: links keep their label, emphasis and code markers are
/// dropped, line breaks become spaces.
pub fn markdown_to_plain_text(text: &str) -> String {
    use comrak::{Arena, Options, nodes::NodeValue, parse_document};

    let arena = Arena::new();
    let root = parse_document(&arena, text, &Options::default());
    let mut plain = String::new();
    for node in root.descendants() {
        match &node.data().value {
            NodeValue::Text(text) => plain.push_str(text),
            NodeValue::Code(code) => plain.push_str(&code.literal),
            NodeValue::HtmlInline(html) => plain.push_str(html),
            NodeValue::SoftBreak | NodeValue::LineBreak => plain.push(' '),
            _ => {}
        }
    }
    plain.trim().to_string()
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

/// Sanitizes third-party HTML (GitHub, Google Calendar, Google Drive) before it
/// is rendered with `dangerous_inner_html`: scripts, event handlers and
/// `javascript:` URLs are removed, links open in a new tab, and read-only
/// checkboxes are kept for GitHub task lists.
pub fn sanitize_html(html: &str) -> String {
    ammonia::Builder::default()
        .set_tag_attribute_value("a", "target", "_blank")
        .add_tags(&["input"])
        .add_tag_attributes("input", &["type", "checked", "disabled"])
        .clean(html)
        .to_string()
}

/// Like [`sanitize_html`], but keeps the `class` of `span` elements, so that
/// the mentions highlighted by `notion-blocks-render` keep their style.
pub fn sanitize_html_keeping_span_classes(html: &str) -> String {
    ammonia::Builder::default()
        .set_tag_attribute_value("a", "target", "_blank")
        .add_tag_attributes("span", &["class"])
        .clean(html)
        .to_string()
}

/// Sanitizes the HTML produced by `slack-blocks-render` before it is rendered
/// with `dangerous_inner_html`. The renderer does not encode quotes in image
/// alt text, so a crafted alt text can inject attributes (`style`, `class`,
/// event handlers) into the generated `<img>`. Only the presentation classes
/// the app passes to the renderer (`text-primary`, `text-warning` on mention
/// `span`s, `slack-emoji` on emoji `img`s) are kept; `style` is never allowed.
pub fn sanitize_slack_html(html: &str) -> String {
    ammonia::Builder::default()
        .set_tag_attribute_value("a", "target", "_blank")
        .add_tag_attributes("span", &["class"])
        .add_tag_attributes("img", &["class"])
        // No sizing, alignment or tooltip attributes: an injected `width`/`height`
        // could still blow an image up over the preview pane
        .rm_tag_attributes("img", &["width", "height", "align"])
        .rm_generic_attributes(&["title"])
        .attribute_filter(|element, attribute, value| match (element, attribute) {
            ("span", "class")
                if value
                    .split_whitespace()
                    .all(|class| matches!(class, "text-primary" | "text-warning")) =>
            {
                Some(value.into())
            }
            ("img", "class") if value == "slack-emoji" => Some(value.into()),
            (_, "class") => None,
            _ => Some(value.into()),
        })
        .clean(html)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn test_sanitize_slack_html_blocks_alt_text_attribute_injection() {
        let sanitized = sanitize_slack_html(
            r#"<p><img src="https://example.com/a.png" alt="x" style="position:fixed" onerror="alert(1)" width="9999" height="9999" align="left" title="t" /></p>"#,
        );

        assert_eq!(
            sanitized,
            r#"<p><img src="https://example.com/a.png" alt="x"></p>"#
        );
    }

    #[wasm_bindgen_test]
    fn test_sanitize_slack_html_neutralizes_renderer_alt_text_injection() {
        use slack_blocks_render::{SlackReferences, html::render_blocks_as_html};
        use slack_morphism::prelude::*;

        let blocks = vec![SlackBlock::Image(SlackImageBlock::new(
            SlackImageUrlOrFile::ImageUrl {
                image_url: Url::parse("https://example.com/a.png").unwrap(),
            },
            r#"x" style="position:fixed" onerror="alert(1)" width="9999"#.to_string(),
        ))];
        let rendered = render_blocks_as_html(
            blocks,
            SlackReferences::default(),
            "text-primary",
            "text-warning",
        );
        // Documents the upstream bug: the renderer really lets attributes through
        assert!(rendered.contains(r#"style="position:fixed""#));

        let sanitized = sanitize_slack_html(&rendered);

        assert!(!sanitized.contains("style="));
        assert!(!sanitized.contains("onerror"));
        assert!(!sanitized.contains("width="));
        assert!(sanitized.contains(r#"src="https://example.com/a.png""#));
    }

    #[wasm_bindgen_test]
    fn test_sanitize_slack_html_filters_classes() {
        let sanitized = sanitize_slack_html(
            r#"<img class="fixed inset-0" src="https://example.com/a.png"><img class="slack-emoji" src="https://example.com/e.png" alt=":x:"><span class="text-warning">@me</span><span class="fixed inset-0">@evil</span><p class="fixed">p</p>"#,
        );

        assert!(!sanitized.contains("fixed"));
        assert!(!sanitized.contains("inset-0"));
        assert!(sanitized.contains(r#"<img class="slack-emoji""#));
        assert!(sanitized.contains(r#"<span class="text-warning">@me</span>"#));
        assert!(sanitized.contains("<span>@evil</span>"));
    }

    #[wasm_bindgen_test]
    fn test_sanitize_slack_html_keeps_allowed_markup() {
        let sanitized = sanitize_slack_html(
            r#"<p><a target="_blank" rel="noopener noreferrer" href="https://example.com">l</a> <em>e</em> <strong>s</strong> <code>c</code> <del>d</del></p><ul><li>i</li></ul><blockquote><p>q</p></blockquote><pre><code>x</code></pre><p><img src="https://example.com/a.png" alt="Image"></p>"#,
        );

        assert!(sanitized.contains(r#"href="https://example.com""#));
        assert!(sanitized.contains(r#"target="_blank""#));
        for fragment in [
            "<em>e</em>",
            "<strong>s</strong>",
            "<code>c</code>",
            "<del>d</del>",
            "<ul><li>i</li></ul>",
            "<blockquote><p>q</p></blockquote>",
            "<pre><code>x</code></pre>",
            r#"<img src="https://example.com/a.png" alt="Image">"#,
        ] {
            assert!(sanitized.contains(fragment), "missing {fragment}");
        }
    }

    #[wasm_bindgen_test]
    fn test_sanitize_slack_html_removes_scripts_and_js_links() {
        let sanitized = sanitize_slack_html(
            r#"<script>alert(1)</script><a href="javascript:alert(1)">link</a><pre style="white-space: pre-wrap;">x</pre>"#,
        );

        assert!(!sanitized.contains("<script"));
        assert!(!sanitized.contains("javascript:"));
        assert!(!sanitized.contains("style="));
    }

    #[wasm_bindgen_test]
    fn test_sanitize_html_removes_scripts() {
        let sanitized = sanitize_html(
            r#"<script>alert(1)</script><p onclick="alert(1)">Safe</p><img src="x" onerror="alert(1)"><a href="javascript:alert(1)">link</a>"#,
        );

        assert!(!sanitized.contains("<script"));
        assert!(!sanitized.contains("onclick"));
        assert!(!sanitized.contains("onerror"));
        assert!(!sanitized.contains("javascript:"));
        assert!(sanitized.contains("<p>Safe</p>"));
    }

    #[wasm_bindgen_test]
    fn test_sanitize_html_keeps_github_task_lists_and_links() {
        let sanitized = sanitize_html(
            r#"<ul><li><input type="checkbox" checked="" disabled="" onchange="alert(1)"> done</li></ul><a href="https://github.com">gh</a>"#,
        );

        assert!(sanitized.contains(r#"<input type="checkbox" checked="" disabled="">"#));
        assert!(!sanitized.contains("onchange"));
        assert!(sanitized.contains(r#"href="https://github.com""#));
        assert!(sanitized.contains(r#"target="_blank""#));
    }

    #[wasm_bindgen_test]
    fn test_markdown_to_plain_text() {
        assert_eq!(
            markdown_to_plain_text(
                "[Add keyboard shortcuts](https://linear.app/issue/UNI-138) for **preview** `pane`"
            ),
            "Add keyboard shortcuts for preview pane"
        );
        assert_eq!(
            markdown_to_plain_text("Fix flaky test in CI pipeline (#812)"),
            "Fix flaky test in CI pipeline (#812)"
        );
        assert_eq!(
            markdown_to_plain_text("line one\nline two"),
            "line one line two"
        );
    }

    #[wasm_bindgen_test]
    fn test_sanitize_html_keeping_span_classes() {
        let sanitized = sanitize_html_keeping_span_classes(
            r#"<span class="text-warning" onclick="alert(1)">@John</span><p class="x">p</p><a href="javascript:alert(1)">link</a>"#,
        );

        assert_eq!(
            sanitized,
            r#"<span class="text-warning">@John</span><p>p</p><a target="_blank" rel="noopener noreferrer">link</a>"#
        );
    }

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
