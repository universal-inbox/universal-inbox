#![allow(non_snake_case)]

use dioxus::prelude::*;

use crate::components::markdown::Markdown;

/// Unified header for every notification preview pane.
///
/// Renders the source brand logo on a neutral 28x28 white tile, the title
/// (with an optional muted identifier such as `#123` or `UNI-13`), an optional
/// subtitle, and a subline that carries the *reason this notification is in the
/// inbox* (state pill, repo, organizer, comment-reply chip, etc.).
///
/// `subtitle` is the slot for a second title that belongs to another system —
/// today, the name a task carries in the user's task manager once they have
/// renamed it away from the one its source seeded. It sits between the title
/// and the subline so both names read as titles rather than metadata.
///
/// `brand_icon` should render to a single `<span>` with an Iconify class
/// so the surrounding `.preview-brand-icon > span` font-size rule applies.
#[component]
pub fn PreviewCardHeader(
    brand_icon: Element,
    title: String,
    identifier: Option<String>,
    title_class: Option<String>,
    #[props(default = None)] subtitle: Option<Element>,
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
                    Markdown {
                        text: "{title}",
                        class: "preview-head-title-text {title_class}",
                    }
                    if let Some(identifier) = identifier {
                        span { class: "preview-head-title-ext", "{identifier}" }
                    }
                }
                if let Some(subtitle) = subtitle {
                    div {
                        class: "preview-head-subtitle",
                        {subtitle}
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
