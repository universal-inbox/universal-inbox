#![allow(non_snake_case)]
#![allow(dead_code)]

//! Centralized [`Button`] component.
//!
//! Uses FlyonUI's `btn` base for Primary/Secondary/Connect/Seed, and
//! design-system tokens via Tailwind utilities for Ghost/Passkey (bordered
//! ghost style: `--ui-border` border, `--ui-surface` bg, `--ui-base-muted`
//! text).
//!
//! ## Variant → class mapping
//!
//! | [`ButtonVariant`]  | Emitted classes                                     |
//! |--------------------|-----------------------------------------------------|
//! | `Primary`          | `btn btn-primary`                                   |
//! | `Secondary`        | `btn btn-soft`                                      |
//! | `Ghost`            | `btn` + bordered ghost utilities                    |
//! | `Tertiary`         | `btn btn-soft btn-xs`                                |
//! | `Danger`           | `btn btn-soft btn-error btn-xs`                     |
//! | `Warning`          | `btn btn-soft btn-warning btn-xs`                   |
//! | `Passkey`          | `btn` + bordered ghost utilities                    |
//! | `Connect`          | `btn btn-primary btn-sm`                            |
//! | `Icon`             | `btn btn-text btn-square btn-xs`                    |
//! | `Seed`             | `btn btn-outline btn-block border-[1.5px] border-dashed` |
//!
//! ## Size mapping
//!
//! | [`ButtonSize`] | Adds     |
//! |----------------|----------|
//! | `Sm`           | `btn-sm` |
//! | `Md` (default) | nothing  |
//! | `Lg`           | `btn-lg` |
//!
//! `Icon` has a fixed size (`btn-xs btn-square`); passing `Sm`/`Lg` is a no-op.
//!
//! ## Render branching
//!
//! Pass `to: Some(Route::…)` to render a `<Link>`, `href: Some(url)` to
//! render an `<a>` (external link), or omit both to render a
//! `<button type="…">`.

use dioxus::prelude::*;

use crate::{
    components::flyonui::tooltip::{Tooltip, TooltipPlacement},
    route::Route,
};

/// Visual variant for [`Button`]. See module-level docs for the variant →
/// class mapping table.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    #[default]
    Primary,
    Ghost,
    Danger,
    Warning,
    /// Rounded icon-only button with hit-area pseudo-element.
    Icon,
}

/// Size token for [`Button`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonSize {
    Xs,
    #[default]
    Sm,
    Md,
    Lg,
}

impl ButtonVariant {
    fn base_classes(self) -> &'static str {
        match self {
            ButtonVariant::Primary => "btn btn-primary rounded-ui-sm ",
            ButtonVariant::Ghost => {
                "btn rounded-ui-sm border border-ui-border bg-ui-surface text-ui-base-muted font-semibold hover:text-ui-base-content hover:border-ui-base-300 hover:bg-ui-surface-hover"
            }
            ButtonVariant::Danger => "btn rounded-ui-sm btn-outline btn-error",
            ButtonVariant::Warning => "btn rounded-ui-sm btn-outline btn-warning",
            ButtonVariant::Icon => {
                "btn btn-text btn-xs btn-circle text-ui-base-muted font-semibold"
            }
        }
    }

    /// `Icon` has a fixed size (`btn-xs btn-square`); size tokens are a no-op.
    fn ignores_size(self) -> bool {
        matches!(self, ButtonVariant::Icon)
    }
}

impl ButtonSize {
    fn size_classes(self, variant: ButtonVariant) -> &'static str {
        if variant.ignores_size() {
            return "";
        }
        match self {
            ButtonSize::Xs => "btn-xs",
            ButtonSize::Sm => "btn-sm",
            ButtonSize::Lg => "btn-lg",
            ButtonSize::Md => "",
        }
    }
}

fn compose_class(
    variant: ButtonVariant,
    size: ButtonSize,
    extra: Option<&str>,
    disabled: bool,
) -> String {
    let base = variant.base_classes();
    let size_cls = size.size_classes(variant);
    let mut out = String::with_capacity(base.len() + size_cls.len() + 24);
    out.push_str(base);
    if !size_cls.is_empty() {
        out.push(' ');
        out.push_str(size_cls);
    }
    if disabled {
        out.push_str(" btn-disabled");
    }
    if let Some(extra) = extra.filter(|e| !e.is_empty()) {
        out.push(' ');
        out.push_str(extra);
    }
    out
}

#[component]
pub fn Button(
    children: Element,
    #[props(default)] variant: ButtonVariant,
    #[props(default)] size: ButtonSize,
    icon_class: Option<String>,
    #[props(default)] onclick: EventHandler<MouseEvent>,
    to: Option<Route>,
    href: Option<String>,
    button_type: Option<String>,
    class: Option<String>,
    title: Option<String>,
    aria_label: Option<String>,
    #[props(default = false)] disabled: bool,
    #[props(optional)] data_overlay: Option<String>,
    #[props(default = false)] enable_tooltip: bool,
    /// Optional DOM id — needed when callers programmatically focus the button
    /// (e.g. `focus_element("task-modal-link-submit")`).
    #[props(optional)]
    id: Option<String>,
) -> Element {
    let class = compose_class(variant, size, class.as_deref(), disabled);
    let icon = icon_class.as_ref().map(|c| {
        rsx! {
            span { class: "{c}" }
        }
    });
    let title_val = title.unwrap_or_default();
    let aria_label_val = aria_label.unwrap_or_default();
    let id_val = id.unwrap_or_default();

    if let Some(route) = to {
        let aria_disabled = if disabled { "true" } else { "false" };
        let tabindex = if disabled { "-1" } else { "0" };
        rsx! {
            Link {
                class: "{class}",
                to: route,
                aria_disabled,
                tabindex,
                title: "{title_val}",
                aria_label: "{aria_label_val}",
                { icon }
                { children }
            }
        }
    } else if let Some(url) = href {
        rsx! {
            a {
                class: "{class}",
                href: "{url}",
                target: "_blank",
                rel: "noopener noreferrer",
                title: "{title_val}",
                "aria-label": "{aria_label_val}",
                { icon }
                { children }
            }
        }
    } else {
        let button_type = button_type.unwrap_or_else(|| "button".to_string());

        if enable_tooltip {
            rsx! {
                Tooltip {
                    class: "flex justify-center",
                    text: "{title_val}",

                    button {
                        class: "{class}",
                        id: "{id_val}",
                        r#type: "{button_type}",
                        disabled,
                        "aria-label": "{aria_label_val}",
                        "data-overlay": "{data_overlay.clone().unwrap_or_default()}",
                        onclick: move |evt| {
                            if !disabled {
                                onclick.call(evt);
                            }
                        },
                        { icon }
                        { children }
                    }
                }
            }
        } else {
            rsx! {
                button {
                    class: "{class}",
                    id: "{id_val}",
                    r#type: "{button_type}",
                    disabled,
                    title: "{title_val}",
                    "aria-label": "{aria_label_val}",
                    "data-overlay": "{data_overlay.clone().unwrap_or_default()}",
                    onclick: move |evt| {
                        if !disabled {
                            onclick.call(evt);
                        }
                    },
                    { icon }
                    { children }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct ActionButtonProps {
    icon_class: String,
    title: ReadSignal<String>,
    shortcut: ReadSignal<String>,
    disabled_label: Option<Option<String>>,
    show_shortcut: ReadSignal<bool>,
    #[props(optional)]
    data_overlay: Option<String>,
    #[props(default)]
    onclick: EventHandler<MouseEvent>,
}

pub fn ActionButton(props: ActionButtonProps) -> Element {
    let shortcut_visibility_style = use_memo(move || {
        if (props.show_shortcut)() {
            "visible"
        } else {
            "invisible group-hover/notification-button:visible"
        }
    });
    let data_overlay = props.data_overlay.clone().unwrap_or_default();

    if let Some(Some(label)) = props.disabled_label {
        rsx! {
            Tooltip {
                class: "flex justify-center",
                tooltip_class: "tooltip-warning",
                text: "{label}",
                placement: TooltipPlacement::Left,

                Button {
                    variant: ButtonVariant::Ghost,
                    disabled: true,
                    aria_label: "{props.title}",
                    icon_class: "{props.icon_class}",
                }
            }
        }
    } else {
        rsx! {
            div {
                class: "relative group/notification-button flex justify-center",

                span {
                    class: "{shortcut_visibility_style} kbd kbd-xs z-50 absolute top-5 left-1.5",
                    "{props.shortcut}"
                }

                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    aria_label: "{props.title}",
                    title: "{props.title}",
                    onclick: move |evt| props.onclick.call(evt),
                    data_overlay: "{data_overlay}",
                    icon_class: "{props.icon_class}",
                    enable_tooltip: true,
                }
            }
        }
    }
}
