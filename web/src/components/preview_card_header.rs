#![allow(non_snake_case)]

use dioxus::prelude::*;

/// Unified header for every notification preview pane.
///
/// Renders the source brand logo on a neutral 28x28 white tile, the title
/// (with an optional muted identifier such as `#123` or `UNI-13`), and a
/// subline that carries the *reason this notification is in the inbox*
/// (state pill, repo, organizer, comment-reply chip, etc.).
///
/// `brand_icon` should render to a single `<span>` with an Iconify class
/// so the surrounding `.preview-brand-icon > span` font-size rule applies.
#[component]
pub fn PreviewCardHeader(
    brand_icon: Element,
    title: String,
    identifier: Option<String>,
    title_class: Option<String>,
    subline: Element,
) -> Element {
    let title_class = title_class.unwrap_or_default();

    rsx! {
        header {
            class: "preview-head",
            span {
                class: "preview-brand-icon",
                "aria-hidden": "true",
                {brand_icon}
            }
            div {
                class: "preview-head-titles",
                div {
                    class: "preview-head-title",
                    span {
                        class: "preview-head-title-text {title_class}",
                        "{title}"
                    }
                    if let Some(identifier) = identifier {
                        span { class: "preview-head-title-ext", "{identifier}" }
                    }
                }
                div {
                    class: "preview-head-sub",
                    {subline}
                }
            }
        }
    }
}
