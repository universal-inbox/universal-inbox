#![allow(non_snake_case)]

use dioxus::prelude::*;

use crate::{
    components::ui::{Button, ButtonSize, ButtonVariant},
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

#[component]
pub fn LegalFooter() -> Element {
    rsx! {
        p { class: "text-xs text-ui-base-muted leading-normal text-center mt-3.5 [&_a]:text-ui-base-muted [&_a]:underline [&_a]:underline-offset-2 [&_a]:hover:text-ui-base-content",
            "By continuing, you agree to our "
            a { href: "https://www.universal-inbox.com/terms-of-service", "Terms" }
            " and "
            a { href: "https://www.universal-inbox.com/privacy-policy", "Privacy Policy" }
            "."
        }
    }
}
