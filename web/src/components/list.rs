#![allow(non_snake_case)]

use std::fmt::Display;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use universal_inbox::{Page, PageToken, task::Task};

use crate::components::{
    integrations::icons::TaskIcon,
    markdown::Markdown,
    ui::button::{Button, ButtonVariant},
};

#[component]
pub fn List(id: String, children: Element) -> Element {
    rsx! {
        div {
            class: "w-full h-max-full",

            { children }
        }
    }
}

#[component]
pub fn ListItem(
    title: ReadSignal<String>,
    subtitle: ReadSignal<Element>,
    time: ReadSignal<String>,
    icon: Option<Element>,
    meta_icon: Option<Element>,
    is_selected: ReadSignal<bool>,
    #[props(default = false)] is_unread: bool,
    #[props(default = None)] linked_task: Option<Task>,
    #[props(default = None)] provider: Option<&'static str>,
    #[props(default = None)] data_kind: Option<&'static str>,
    on_select: EventHandler<()>,
) -> Element {
    let selected_class = use_memo(move || if is_selected() { "selected" } else { "" });
    let unread_class = if is_unread { "unread" } else { "" };
    let title_class = if is_unread {
        "ui-nrow-title b"
    } else {
        "ui-nrow-title"
    };

    rsx! {
        div {
            class: "ui-nrow {selected_class} {unread_class} snap-start max-md:py-2.5 max-md:px-3.5",
            "data-provider": provider,
            "data-kind": data_kind,
            onclick: move |_| {
                if !is_selected() {
                    on_select.call(());
                }
            },

            div {
                class: "ui-nrow-source",
                if let Some(icon) = icon {
                    { icon }
                }
                if let Some(task) = linked_task.as_ref() {
                    span {
                        class: "ui-nrow-source-corner",
                        title: "Linked task",
                        TaskIcon { class: "h-2 w-2".to_string(), kind: task.kind }
                    }
                }
            }

            div {
                class: "ui-nrow-body",

                div {
                    class: "ui-nrow-title-row",
                    span {
                        class: "{title_class} max-md:text-sm",
                        Markdown { text: "{title}" }
                    }
                    span {
                        class: "flex items-center gap-1 ml-auto flex-shrink-0",
                        if is_unread {
                            span { class: "ui-nrow-unread-dot" }
                        }
                        span { class: "ui-nrow-time", "{time}" }
                    }
                }

                div {
                    class: "ui-nrow-meta",
                    if let Some(meta_icon) = meta_icon {
                        span { class: "ui-nrow-meta-icon", { meta_icon } }
                    }
                    { subtitle() }
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
        current_page_style,
        next_pages_style,
        last_page_style,
        next_button_style,
    } = compute_list_pagination_buttons_style(current_page(), page().pages_count);

    rsx! {
        nav {
            class: "join",

            Button {
                variant: ButtonVariant::Icon,
                disabled: previous_button_style == ButtonStyle::Disabled,
                aria_label: "Previous page".to_string(),
                class: "text-ui-base-muted",
                onclick: move |_| {
                    current_page -= 1;
                    // Offset-based (like the numbered page buttons) so pagination is
                    // robust to notifications sharing an `updated_at` timestamp, which
                    // a strict cursor (`previous_page_token`) would skip.
                    on_select.call(PageToken::Offset((current_page() - 1) * page().per_page));
                },
                icon_class: "icon-[tabler--chevron-left]".to_string(),
            }

            button {
                "type": "button",
                class: "btn btn-text btn-xs join-item btn-circle text-ui-base-muted aria-[current='page']:text-bg-soft-primary",
                "aria-current": if current_page() == 1 { "page" },
                onclick: move |_| {
                    current_page.set(1);
                    on_select.call(PageToken::Offset(0));
                },
                "1"
            }

            button {
                "type": "button",
                class: "btn btn-text btn-xs join-item btn-circle text-ui-base-muted {previous_pages_style}",
                onclick: move |_| {
                    current_page -= 2;
                    on_select.call(PageToken::Offset((current_page() - 1) * page().per_page));
                },
                "..."
            }

            button {
                "type": "button",
                class: "btn btn-text btn-xs join-item btn-circle text-ui-base-muted aria-[current='page']:text-bg-soft-primary {current_page_style}",
                "aria-current": "page",
                "{current_page()}"
            }

            button {
                "type": "button",
                class: "btn btn-text btn-xs join-item btn-circle text-ui-base-muted {next_pages_style}",
                onclick: move |_| {
                    current_page += 2;
                    on_select.call(PageToken::Offset((current_page() - 1) * page().per_page));
                },
                "..."
            }

            button {
                "type": "button",
                class: "btn btn-text btn-xs join-item btn-circle text-ui-base-muted aria-[current='page']:text-bg-soft-primary {last_page_style}",
                "aria-current": if current_page() == page().pages_count { "page" },
                onclick: move |_| {
                    current_page.set(page().pages_count);
                    on_select.call(PageToken::Offset((current_page() - 1) * page().per_page));
                },
                "{page().pages_count}"
            }

            Button {
                variant: ButtonVariant::Icon,
                disabled: next_button_style == ButtonStyle::Disabled,
                aria_label: "Next page".to_string(),
                class: "text-ui-base-muted btn-circle",
                onclick: move |_| {
                    current_page += 1;
                    // Offset-based (like the numbered page buttons) so pagination is
                    // robust to notifications sharing an `updated_at` timestamp, which
                    // a strict cursor (`next_page_token`) would skip.
                    on_select.call(PageToken::Offset((current_page() - 1) * page().per_page));
                },
                icon_class: "icon-[tabler--chevron-right]".to_string(),
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
    current_page_style: ButtonStyle,
    next_pages_style: ButtonStyle,
    last_page_style: ButtonStyle,
    next_button_style: ButtonStyle,
}

fn compute_list_pagination_buttons_style(
    current_page: usize,
    pages_count: usize,
) -> ListPaginationButtonsStyle {
    let current_page_visible = current_page >= 2 && current_page < pages_count;
    let previous_pages_visible = if current_page_visible {
        current_page >= 3
    } else {
        current_page == pages_count && pages_count >= 3
    };
    let next_pages_visible = if current_page_visible {
        current_page + 1 < pages_count
    } else {
        current_page == 1 && pages_count >= 3
    };

    ListPaginationButtonsStyle {
        previous_button_style: if current_page == 1 {
            ButtonStyle::Disabled
        } else {
            ButtonStyle::None
        },
        previous_pages_style: if previous_pages_visible {
            ButtonStyle::Visible
        } else {
            ButtonStyle::Hidden
        },
        current_page_style: if current_page_visible {
            ButtonStyle::Visible
        } else {
            ButtonStyle::Hidden
        },
        next_pages_style: if next_pages_visible {
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
        assert_eq!(style.current_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.last_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_button_style, ButtonStyle::Disabled);
    }

    #[wasm_bindgen_test]
    fn test_compute_list_pagination_buttons_style_with_2_pages() {
        let style = compute_list_pagination_buttons_style(1, 2);
        assert_eq!(style.previous_button_style, ButtonStyle::Disabled);
        assert_eq!(style.previous_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.current_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::None);

        let style = compute_list_pagination_buttons_style(2, 2);
        assert_eq!(style.previous_button_style, ButtonStyle::None);
        assert_eq!(style.previous_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.current_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::Disabled);
    }

    #[wasm_bindgen_test]
    fn test_compute_list_pagination_buttons_style_with_3_pages() {
        let style = compute_list_pagination_buttons_style(1, 3);
        assert_eq!(style.previous_button_style, ButtonStyle::Disabled);
        assert_eq!(style.previous_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.current_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_pages_style, ButtonStyle::Visible);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::None);

        let style = compute_list_pagination_buttons_style(2, 3);
        assert_eq!(style.previous_button_style, ButtonStyle::None);
        assert_eq!(style.previous_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.current_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::None);

        let style = compute_list_pagination_buttons_style(3, 3);
        assert_eq!(style.previous_button_style, ButtonStyle::None);
        assert_eq!(style.previous_pages_style, ButtonStyle::Visible);
        assert_eq!(style.current_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::Disabled);
    }

    #[wasm_bindgen_test]
    fn test_compute_list_pagination_buttons_style_with_4_pages() {
        let style = compute_list_pagination_buttons_style(1, 4);
        assert_eq!(style.previous_button_style, ButtonStyle::Disabled);
        assert_eq!(style.previous_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.current_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_pages_style, ButtonStyle::Visible);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::None);

        let style = compute_list_pagination_buttons_style(2, 4);
        assert_eq!(style.previous_button_style, ButtonStyle::None);
        assert_eq!(style.previous_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.current_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_pages_style, ButtonStyle::Visible);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::None);

        let style = compute_list_pagination_buttons_style(3, 4);
        assert_eq!(style.previous_button_style, ButtonStyle::None);
        assert_eq!(style.previous_pages_style, ButtonStyle::Visible);
        assert_eq!(style.current_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::None);

        let style = compute_list_pagination_buttons_style(4, 4);
        assert_eq!(style.previous_button_style, ButtonStyle::None);
        assert_eq!(style.previous_pages_style, ButtonStyle::Visible);
        assert_eq!(style.current_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::Disabled);
    }

    #[wasm_bindgen_test]
    fn test_compute_list_pagination_buttons_style_with_5_pages() {
        let style = compute_list_pagination_buttons_style(1, 5);
        assert_eq!(style.previous_button_style, ButtonStyle::Disabled);
        assert_eq!(style.previous_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.current_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_pages_style, ButtonStyle::Visible);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::None);

        let style = compute_list_pagination_buttons_style(2, 5);
        assert_eq!(style.previous_button_style, ButtonStyle::None);
        assert_eq!(style.previous_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.current_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_pages_style, ButtonStyle::Visible);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::None);

        let style = compute_list_pagination_buttons_style(3, 5);
        assert_eq!(style.previous_button_style, ButtonStyle::None);
        assert_eq!(style.previous_pages_style, ButtonStyle::Visible);
        assert_eq!(style.current_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_pages_style, ButtonStyle::Visible);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::None);

        let style = compute_list_pagination_buttons_style(4, 5);
        assert_eq!(style.previous_button_style, ButtonStyle::None);
        assert_eq!(style.previous_pages_style, ButtonStyle::Visible);
        assert_eq!(style.current_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::None);

        let style = compute_list_pagination_buttons_style(5, 5);
        assert_eq!(style.previous_button_style, ButtonStyle::None);
        assert_eq!(style.previous_pages_style, ButtonStyle::Visible);
        assert_eq!(style.current_page_style, ButtonStyle::Hidden);
        assert_eq!(style.next_pages_style, ButtonStyle::Hidden);
        assert_eq!(style.last_page_style, ButtonStyle::Visible);
        assert_eq!(style.next_button_style, ButtonStyle::Disabled);
    }
}
