#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsArrowUpRightSquare, Icon};
use url::Url;

use crate::components::{
    flyonui::tooltip::{Tooltip, TooltipPlacement},
    markdown::Markdown,
};

#[derive(Clone, PartialEq)]
pub struct ListContext {
    pub show_shortcut: bool,
}

#[component]
pub fn List(id: String, show_shortcut: ReadOnlySignal<bool>, children: Element) -> Element {
    let context = use_memo(move || ListContext {
        show_shortcut: show_shortcut(),
    });
    use_context_provider(move || context);

    rsx! {
        table {
            class: "table table-pin-rows w-full h-max-full",

            { children }
        }
    }
}

#[component]
pub fn ListItem(
    title: ReadOnlySignal<String>,
    subtitle: ReadOnlySignal<Element>,
    link: ReadOnlySignal<Url>,
    icon: Option<Element>,
    subicon: Option<Element>,
    is_selected: ReadOnlySignal<bool>,
    on_select: EventHandler<()>,
    action_buttons: ReadOnlySignal<Vec<Element>>,
    children: Element,
) -> Element {
    let style = use_memo(move || if is_selected() { "row-active" } else { "" });
    let list_context = use_context::<Memo<ListContext>>();
    let shortcut_visibility_style = use_memo(move || {
        if is_selected() && list_context().show_shortcut {
            "visible"
        } else {
            "invisible"
        }
    });
    let (button_active_style, details_style, button_style) = use_memo(move || {
        if is_selected() {
            ("swap-active", "invisible", "")
        } else {
            ("", "", "invisible")
        }
    })();

    rsx! {
        tr {
            class: "row-hover flex items-center py-1 {style} group snap-start cursor-pointer",
            onclick: move |_| {
                if !is_selected() {
                    on_select.call(());
                }
            },

            td {
                class: "flex items-center px-2 py-0 rounded-none relative h-12 relative",
                span {
                    class: "{shortcut_visibility_style} kbd kbd-xs z-50 absolute bottom-10",
                    "▲"
                }
                span {
                    class: "{shortcut_visibility_style} kbd kbd-xs z-50 absolute top-10",
                    "▼"
                }

                if let Some(icon) = icon {
                    div {
                        class: "flex justify-center items-center h-8 w-8",
                        { icon }
                    }
                } else {
                    div { class: "flex flex-col h-5 w-5 min-w-5" }
                }
            }

            td {
                class: "px-2 py-0 grow whitespace-normal",

                div {
                    class: "flex items-center gap-2",

                    if let Some(subicon) = subicon {
                        div {
                            class: "flex justify-center items-center h-8 w-8",
                            { subicon }
                        }
                    } else {
                        div { class: "flex flex-col h-5 w-5 min-w-5" }
                    }

                    div {
                        class: "flex flex-col grow",

                        div {
                            class: "flex",
                            a {
                                class: "flex items-center max-lg:hidden",
                                href: "{link}",
                                target: "_blank",
                                Markdown { text: "{title}" }
                                Icon { class: "h-5 w-5 min-w-5 text-base-content/50 p-1", icon: BsArrowUpRightSquare }
                            }
                            Markdown { class: "lg:hidden", text: "{title}" }
                            div { class: "grow" }
                        }

                        { subtitle() }
                    }
                }
            }

            td {
                class: "px-2 py-0 rounded-none flex items-center justify-end max-lg:hidden",
                div {
                    class: "swap {button_active_style}",
                    // Buttons
                    div {
                        class: "swap-on flex items-center justify-end {button_style}",
                        for button in action_buttons() {
                            { button }
                        }
                    }
                    // Details
                    div {
                        class: "swap-off text-xs flex gap-2 items-center justify-end {details_style}",

                        { children }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct ListItemActionButtonProps {
    children: Element,
    title: ReadOnlySignal<String>,
    shortcut: ReadOnlySignal<String>,
    disabled_label: Option<Option<String>>,
    button_class: Option<String>,
    container_class: Option<String>,
    show_shortcut: ReadOnlySignal<bool>,
    #[props(optional)]
    data_overlay: Option<String>,
    #[props(optional)]
    onclick: Option<EventHandler<MouseEvent>>,
}

pub fn ListItemActionButton(props: ListItemActionButtonProps) -> Element {
    let shortcut_visibility_style = use_memo(move || {
        if (props.show_shortcut)() {
            "visible"
        } else {
            "invisible group-hover/notification-button:visible"
        }
    });
    let data_overlay = props.data_overlay.clone().unwrap_or_default();
    let button_class = props
        .button_class
        .unwrap_or_else(|| "btn btn-text btn-square btn-sm".to_string());
    let container_class = props.container_class.unwrap_or_default();

    if let Some(Some(label)) = props.disabled_label {
        rsx! {
            Tooltip {
                class: "flex justify-center {container_class}",
                tooltip_class: "tooltip-warning",
                text: "{label}",
                placement: TooltipPlacement::Left,

                button {
                    class: "{button_class} btn-disabled",
                    title: "{props.title}",

                    { props.children }
                }
            }
        }
    } else {
        rsx! {
            div {
                class: "relative group/notification-button flex justify-center {container_class}",

                span {
                    class: "{shortcut_visibility_style} kbd kbd-xs z-50 absolute top-5 left-1.5",
                    "{props.shortcut}"
                }

                button {
                    class: "{button_class}",
                    title: "{props.title}",
                    "data-overlay": "{data_overlay}",
                    onclick: move |evt| {
                        if let Some(handler) = &props.onclick {
                            handler.call(evt)
                        }
                    },

                    { props.children }
                }
            }
        }
    }
}
