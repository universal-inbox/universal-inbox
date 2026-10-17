#![allow(non_snake_case)]

use dioxus::prelude::*;

use crate::{
    components::ui::{Button, ButtonSize, ButtonVariant},
    config::APP_CONFIG,
    icons::GOOGLE_LOGO,
    route::Route,
};

#[component]
pub fn PrimaryBtn(
    children: Element,
    icon_class: Option<String>,
    button_type: Option<String>,
    #[props(default)] onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        Button {
            variant: ButtonVariant::Primary,
            size: ButtonSize::Md,
            class: "btn-block".to_string(),
            button_type,
            icon_class,
            onclick: move |evt| onclick.call(evt),
            { children }
        }
    }
}

#[component]
pub fn PasskeyBtn(children: Element, to: Option<Route>) -> Element {
    rsx! {
        Button {
            variant: ButtonVariant::Ghost,
            size: ButtonSize::Md,
            class: "btn-block".to_string(),
            to,
            button_type: "submit".to_string(),
            icon_class: "icon-[lucide--fingerprint]".to_string(),
            { children }
        }
    }
}

#[component]
pub fn GoogleBtn(children: Element, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        Button {
            variant: ButtonVariant::Ghost,
            size: ButtonSize::Md,
            class: "btn-block".to_string(),
            onclick: move |evt| onclick.call(evt),
            img {
                class: "size-5",
                src: "{GOOGLE_LOGO}",
                alt: "Google",
            }
            { children }
        }
    }
}

#[component]
pub fn AuthDivider(label: String) -> Element {
    rsx! {
        div { class: "auth-divider", "{label}" }
    }
}

#[component]
pub fn Backlink(to: Route, children: Element) -> Element {
    rsx! {
        Link {
            class: "inline-flex items-center gap-1.5 text-xs font-semibold text-ui-base-muted no-underline mb-6 hover:text-ui-base-content",
            to,

            span { class: "icon-[lucide--arrow-left] size-4" }
            { children }
        }
    }
}

/// The legal links to render, in display order, as `(label, href)` pairs.
///
/// Both URLs come from the instance configuration and are unset by default, so
/// a self-hosted instance never advertises universal-inbox.com's documents as
/// its own. A link with no URL is dropped; an empty result means the footer
/// renders nothing at all.
pub fn legal_links(
    terms_href: Option<String>,
    privacy_policy_href: Option<String>,
) -> Vec<(&'static str, String)> {
    [
        ("Terms", terms_href),
        ("Privacy Policy", privacy_policy_href),
    ]
    .into_iter()
    .filter_map(|(label, href)| href.map(|href| (label, href)))
    .collect()
}

#[component]
pub fn LegalFooter() -> Element {
    let links = APP_CONFIG
        .read()
        .as_ref()
        .map(|config| {
            legal_links(
                config.terms_href.clone(),
                config.privacy_policy_href.clone(),
            )
        })
        .unwrap_or_default();

    if links.is_empty() {
        return rsx! { Fragment {} };
    }

    rsx! {
        p { class: "text-xs text-ui-base-muted leading-normal text-center mt-3.5 [&_a]:text-ui-base-muted [&_a]:underline [&_a]:underline-offset-2 [&_a]:hover:text-ui-base-content",
            "By continuing, you agree to our "
            for (index, (label, href)) in links.into_iter().enumerate() {
                if index > 0 {
                    " and "
                }
                a {
                    href: "{href}",
                    target: "_blank",
                    rel: "noopener noreferrer",
                    "{label}"
                }
            }
            "."
        }
    }
}

#[cfg(test)]
mod legal_links_tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn test_no_link_when_nothing_is_configured() {
        assert_eq!(legal_links(None, None), vec![]);
    }

    #[wasm_bindgen_test]
    fn test_only_the_configured_link_is_kept() {
        assert_eq!(
            legal_links(None, Some("https://example.com/privacy".to_string())),
            vec![("Privacy Policy", "https://example.com/privacy".to_string())]
        );
        assert_eq!(
            legal_links(Some("https://example.com/terms".to_string()), None),
            vec![("Terms", "https://example.com/terms".to_string())]
        );
    }

    #[wasm_bindgen_test]
    fn test_terms_come_before_the_privacy_policy() {
        assert_eq!(
            legal_links(
                Some("https://example.com/terms".to_string()),
                Some("https://example.com/privacy".to_string())
            ),
            vec![
                ("Terms", "https://example.com/terms".to_string()),
                ("Privacy Policy", "https://example.com/privacy".to_string())
            ]
        );
    }
}
