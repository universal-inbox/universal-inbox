#![allow(non_snake_case)]

use std::fmt::Display;

use dioxus::prelude::*;
use dioxus_free_icons::{Icon, icons::bs_icons::BsArrowUpRightSquare};
use serde::{Deserialize, Serialize};
use url::Url;

use universal_inbox::{Page, PageToken};

use crate::components::{
    flyonui::tooltip::{Tooltip, TooltipPlacement},
    markdown::Markdown,
};

#[derive(Clone, PartialEq)]
pub struct ListContext {
    pub show_shortcut: bool,
}

#[component]
pub fn List(id: String, show_shortcut: ReadSignal<bool>, children: Element) -> Element {
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
    title: ReadSignal<String>,
    subtitle: ReadSignal<Element>,
    link: ReadSignal<Url>,
    icon: Option<Element>,
    subicon: Option<Element>,
    is_selected: ReadSignal<bool>,
    on_select: EventHandler<()>,
    action_buttons: ReadSignal<Vec<Element>>,
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
    title: ReadSignal<String>,
    shortcut: ReadSignal<String>,
    disabled_label: Option<Option<String>>,
    button_class: Option<String>,
    container_class: Option<String>,
    show_shortcut: ReadSignal<bool>,
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

#[component]
pub fn ListPaginationButtons<
    T: Serialize + for<'d> Deserialize<'d> + 'static + Clone + PartialEq,
>(
    current_page: Signal<usize>,
    page: ReadSignal<Page<T>>,
    on_select: EventHandler<PageToken>,
) -> Element {
    if page().pages_count == 0 {
        return rsx! {};
    }

    let ListPaginationButtonsStyle {
        previous_button_style,
        previous_pages_style,
        previous_page_style,
        current_page_style,
        next_page_style,
        next_pages_style,
        last_page_style,
        next_button_style,
    } = compute_list_pagination_buttons_style(current_page(), page().pages_count);

    rsx! {
        nav {
            class: "join",

            button {
                "type": "button",
                class: "btn btn-text lg:btn-xs max-lg:btn-lg btn-circle join-item {previous_button_style}",
                "aria-label": "Previous page",
                onclick: move |_| {
                    current_page -= 1;
                    on_select.call(page().previous_page_token.unwrap_or_default());
                },
                span { class: "icon-[tabler--chevron-left] size-5 rtl:rotate-180" }
            }
            button {
                "type": "button",
                class: "btn btn-text lg:btn-xs max-lg:btn-lg join-item btn-circle aria-[current='page']:text-bg-soft-primary",
                "aria-current": if current_page() == 1 { "page" },
                onclick: move |_| {
                    current_page.set(1);
                    on_select.call(PageToken::Offset(0));
                },
                "1"
            }

            button {
                "type": "button",
                class: "btn btn-text lg:btn-xs max-lg:btn-lg join-item btn-circle {previous_pages_style}",
                onclick: move |_| {
                    current_page -= 2;
                    on_select.call(PageToken::Offset((current_page() - 1) * page().per_page));
                },
                "..."
            }

            button {
                "type": "button",
                class: "btn btn-text lg:btn-xs max-lg:btn-lg join-item btn-circle {previous_page_style}",
                onclick: move |_| {
                    current_page -= 1;
                    on_select.call(page().previous_page_token.unwrap_or_default());
                },
                "{current_page() - 1}"
            }
            button {
                "type": "button",
                class: "btn btn-text lg:btn-xs max-lg:btn-lg join-item btn-circle aria-[current='page']:text-bg-soft-primary {current_page_style}",
                "aria-current": "page",
                "{current_page()}"
            }
            button {
                "type": "button",
                class: "btn btn-text lg:btn-xs max-lg:btn-lg join-item btn-circle {next_page_style}",
                onclick: move |_| {
                    current_page += 1;
                    on_select.call(page().next_page_token.unwrap_or_default());
                },
                "{current_page() + 1}"
            }

            button {
                "type": "button",
                class: "btn btn-text lg:btn-xs max-lg:btn-lg join-item btn-circle {next_pages_style}",
                onclick: move |_| {
                    current_page += 2;
                    on_select.call(PageToken::Offset((current_page() - 1) * page().per_page));
                },
                "..."
            }

            button {
                "type": "button",
                class: "btn btn-text lg:btn-xs max-lg:btn-lg join-item btn-circle aria-[current='page']:text-bg-soft-primary {last_page_style}",
                "aria-current": if current_page() == page().pages_count { "page" },
                onclick: move |_| {
                    current_page.set(page().pages_count);
                    on_select.call(PageToken::Offset((current_page() - 1) * page().per_page));
                },
                "{page().pages_count}"
            }
            button {
                "type": "button",
                class: "btn btn-text lg:btn-xs max-lg:btn-lg btn-circle join-item {next_button_style}",
                "aria-label": "Next page",
                onclick: move |_| {
                    current_page += 1;
                    on_select.call(page().next_page_token.unwrap_or_default());
                },
                span { class: "icon-[tabler--chevron-right] size-5 rtl:rotate-180" }
            }
        }
    }
}

#[derive(PartialEq, Clone, Debug)]
enum ButtonStyle {
    Disabled,
    Visible,
    Hidden,
    None,
}

impl Display for ButtonStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ButtonStyle::Disabled => write!(f, "btn-disabled"),
            ButtonStyle::Visible => write!(f, "visible"),
            ButtonStyle::Hidden => write!(f, "hidden"),
            ButtonStyle::None => write!(f, ""),
        }
    }
}

#[derive(PartialEq, Clone, Debug)]
struct ListPaginationButtonsStyle {
    previous_button_style: ButtonStyle,
    previous_pages_style: ButtonStyle,
    previous_page_style: ButtonStyle,
    current_page_style: ButtonStyle,
    next_page_style: ButtonStyle,
    next_pages_style: ButtonStyle,
    last_page_style: ButtonStyle,
    next_button_style: ButtonStyle,
}

fn compute_list_pagination_buttons_style(
    current_page: usize,
    pages_count: usize,
) -> ListPaginationButtonsStyle {
    ListPaginationButtonsStyle {
        previous_button_style: if current_page == 1 {
            ButtonStyle::Disabled
        } else {
            ButtonStyle::None
        },
        previous_pages_style: if current_page >= 4 {
            ButtonStyle::Visible
        } else {
            ButtonStyle::Hidden
        },
        previous_page_style: if current_page >= 3 {
            ButtonStyle::Visible
        } else {
            ButtonStyle::Hidden
        },
        current_page_style: if current_page >= 2 && current_page <= (pages_count - 1) {
            ButtonStyle::Visible
        } else {
            ButtonStyle::Hidden
        },
        next_page_style: if pages_count >= 3 && current_page <= (pages_count - 2) {
            ButtonStyle::Visible
        } else {
            ButtonStyle::Hidden
        },
        next_pages_style: if pages_count >= 4 && current_page <= (pages_count - 3) {
            ButtonStyle::Visible
        } else {
            ButtonStyle::Hidden
        },
        last_page_style: if pages_count >= 2 {
            ButtonStyle::Visible
        } else {
            ButtonStyle::Hidden
        },
        next_button_style: if current_page == pages_count {
            ButtonStyle::Disabled
        } else {
            ButtonStyle::None
        },
    }
}

#[cfg(test)]
mod tests_list_pagination_buttons {
    use super::*;

    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn test_compute_list_pagination_buttons_style_with_a_single_page() {
        let style = compute_list_pagination_buttons_style(1, 1);
        assert_eq!(style.previous_button_style, ButtonStyle::Disabled);
        assert_eq!(style.previous_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.previous_page_style, ButtonStyle::Hidden);
        assert_eq!(style.current_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.last_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_button_style, ButtonStyle::Disabled);
    }

    #[wasm_bindgen_test]
    fn test_compute_list_pagination_buttons_style_with_2_pages() {
        let style = compute_list_pagination_buttons_style(1, 2);
        assert_eq!(style.previous_button_style, ButtonStyle::Disabled);
        assert_eq!(style.previous_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.previous_page_style, ButtonStyle::Hidden);
        assert_eq!(style.current_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::None);

        let style = compute_list_pagination_buttons_style(2, 2);
        assert_eq!(style.previous_button_style, ButtonStyle::None);
        assert_eq!(style.previous_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.previous_page_style, ButtonStyle::Hidden);
        assert_eq!(style.current_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::Disabled);
    }

    #[wasm_bindgen_test]
    fn test_compute_list_pagination_buttons_style_with_3_pages() {
        let style = compute_list_pagination_buttons_style(1, 3);
        assert_eq!(style.previous_button_style, ButtonStyle::Disabled);
        assert_eq!(style.previous_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.previous_page_style, ButtonStyle::Hidden);
        assert_eq!(style.current_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::None);

        let style = compute_list_pagination_buttons_style(2, 3);
        assert_eq!(style.previous_button_style, ButtonStyle::None);
        assert_eq!(style.previous_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.previous_page_style, ButtonStyle::Hidden);
        assert_eq!(style.current_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::None);

        let style = compute_list_pagination_buttons_style(3, 3);
        assert_eq!(style.previous_button_style, ButtonStyle::None);
        assert_eq!(style.previous_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.previous_page_style, ButtonStyle::Visible);
        assert_eq!(style.current_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::Disabled);
    }

    #[wasm_bindgen_test]
    fn test_compute_list_pagination_buttons_style_with_4_pages() {
        let style = compute_list_pagination_buttons_style(1, 4);
        assert_eq!(style.previous_button_style, ButtonStyle::Disabled);
        assert_eq!(style.previous_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.previous_page_style, ButtonStyle::Hidden);
        assert_eq!(style.current_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_pages_style, ButtonStyle::Visible);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::None);

        let style = compute_list_pagination_buttons_style(2, 4);
        assert_eq!(style.previous_button_style, ButtonStyle::None);
        assert_eq!(style.previous_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.previous_page_style, ButtonStyle::Hidden);
        assert_eq!(style.current_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::None);

        let style = compute_list_pagination_buttons_style(3, 4);
        assert_eq!(style.previous_button_style, ButtonStyle::None);
        assert_eq!(style.previous_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.previous_page_style, ButtonStyle::Visible);
        assert_eq!(style.current_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::None);

        let style = compute_list_pagination_buttons_style(4, 4);
        assert_eq!(style.previous_button_style, ButtonStyle::None);
        assert_eq!(style.previous_pages_style, ButtonStyle::Visible);
        assert_eq!(style.previous_page_style, ButtonStyle::Visible);
        assert_eq!(style.current_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::Disabled);
    }

    #[wasm_bindgen_test]
    fn test_compute_list_pagination_buttons_style_with_5_pages() {
        let style = compute_list_pagination_buttons_style(1, 5);
        assert_eq!(style.previous_button_style, ButtonStyle::Disabled);
        assert_eq!(style.previous_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.previous_page_style, ButtonStyle::Hidden);
        assert_eq!(style.current_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_pages_style, ButtonStyle::Visible);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::None);

        let style = compute_list_pagination_buttons_style(2, 5);
        assert_eq!(style.previous_button_style, ButtonStyle::None);
        assert_eq!(style.previous_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.previous_page_style, ButtonStyle::Hidden);
        assert_eq!(style.current_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_pages_style, ButtonStyle::Visible);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::None);

        let style = compute_list_pagination_buttons_style(3, 5);
        assert_eq!(style.previous_button_style, ButtonStyle::None);
        assert_eq!(style.previous_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.previous_page_style, ButtonStyle::Visible);
        assert_eq!(style.current_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::None);

        let style = compute_list_pagination_buttons_style(4, 5);
        assert_eq!(style.previous_button_style, ButtonStyle::None);
        assert_eq!(style.previous_pages_style, ButtonStyle::Visible);
        assert_eq!(style.previous_page_style, ButtonStyle::Visible);
        assert_eq!(style.current_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::None);

        let style = compute_list_pagination_buttons_style(5, 5);
        assert_eq!(style.previous_button_style, ButtonStyle::None);
        assert_eq!(style.previous_pages_style, ButtonStyle::Visible);
        assert_eq!(style.previous_page_style, ButtonStyle::Visible);
        assert_eq!(style.current_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::Disabled);
    }
}
