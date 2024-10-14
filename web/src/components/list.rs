#![allow(non_snake_case)]

use dioxus::prelude::*;

use crate::components::markdown::Markdown;

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
            class: "table w-full h-max-full",

            tbody { { children } }
        }
    }
}

#[component]
pub fn ListItem(
    title: ReadOnlySignal<String>,
    subtitle: ReadOnlySignal<Element>,
    icon: Option<Element>,
    subicon: Option<Element>,
    is_selected: ReadOnlySignal<bool>,
    on_select: EventHandler<()>,
    action_buttons: ReadOnlySignal<Vec<Element>>,
    children: Element,
) -> Element {
    let style = use_memo(move || if is_selected() { "active" } else { "" });
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
            class: "hover flex items-center py-1 {style} group snap-start cursor-pointer",
            onclick: move |_| {
                if !is_selected() {
                    on_select.call(());
                }
            },

            td {
                class: "flex items-center px-2 py-0 rounded-none relative h-12 indicator",
                span {
                    class: "{shortcut_visibility_style} indicator-item indicator-top indicator-start badge text-xs text-gray-400 z-50",
                    "▲"
                }
                span {
                    class: "{shortcut_visibility_style} indicator-item indicator-bottom indicator-start badge text-xs text-gray-400 z-50",
                    "▼"
                }

                if let Some(icon) = icon {
                    div {
                        class: "flex justify-center",
                        { icon }
                    }
                } else {
                    div { class: "flex flex-col h-5 w-5 min-w-5" }
                }
            }

            td {
                class: "px-2 py-0 grow",

                div {
                    class: "flex items-center gap-2",

                    if let Some(subicon) = subicon {
                        div {
                            class: "flex justify-center",
                            { subicon }
                        }
                    } else {
                        div { class: "flex flex-col h-5 w-5 min-w-5" }
                    }

                    div {
                        class: "flex flex-col grow",

                        Markdown { text: "{title}" }

                        { subtitle() }
                    }
                }
            }

            td {
                class: "px-2 py-0 rounded-none flex items-center justify-end",
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
    show_shortcut: ReadOnlySignal<bool>,
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

    if let Some(Some(label)) = props.disabled_label {
        rsx! {
            div {
                class: "tooltip tooltip-left text-xs text-gray-400",
                "data-tip": "{label}",

                button {
                    class: "btn btn-ghost btn-square btn-disabled",
                    title: "{props.title}",

                    { props.children }
                }
            }
        }
    } else {
        rsx! {
            div {
                class: "indicator group/notification-button",

                span {
                    class: "{shortcut_visibility_style} indicator-item indicator-bottom indicator-center badge text-xs text-gray-400 z-50",
                    "{props.shortcut}"
                }

                button {
                    class: "btn btn-ghost btn-square",
                    title: "{props.title}",
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
