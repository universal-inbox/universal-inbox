#![allow(non_snake_case)]

use std::{fmt::Display, marker::PhantomData, str::FromStr};

use dioxus::{html::input_data::keyboard_types::Key, prelude::*};
use dioxus_free_icons::{icons::bs_icons::BsSearch, Icon};
use log::error;
use wasm_bindgen::{prelude::Closure, JsCast};
use web_sys::KeyboardEvent;

use crate::utils::{focus_and_select_input_element, wait_for_element_by_id};

#[derive(Props, Clone, PartialEq)]
pub struct InputProps<T: Clone + PartialEq + 'static> {
    name: ReadOnlySignal<String>,
    #[props(!optional)]
    label: ReadOnlySignal<Option<String>>,
    required: Option<bool>,
    value: Signal<String>,
    #[props(default)]
    autofocus: Option<bool>,
    #[props(default)]
    force_validation: Option<bool>,
    #[props(default)]
    r#type: Option<String>,
    #[props(default)]
    icon: Option<Element>,
    #[props(default)]
    phantom: PhantomData<T>,
}

const INPUT_INVALID_STYLE: &str = "border-error focus:border-error";
const FLOATING_LABEL_INVALID_STYLE: &str = "text-error peer-focus:text-error";

#[component]
pub fn FloatingLabelInputText<T>(mut props: InputProps<T>) -> Element
where
    T: FromStr + Clone + PartialEq,
    <T as FromStr>::Err: Display,
{
    let required = props.required.unwrap_or_default();
    let required_label_style = required
        .then_some("after:content-['*'] after:ml-0.5 after:text-error")
        .unwrap_or_default();

    let error_message = use_signal(|| None);
    let icon = props.icon.clone();
    let input_style = use_memo(move || {
        to_owned![icon];
        format!(
            "{} {}",
            error_message()
                .and(Some(INPUT_INVALID_STYLE))
                .unwrap_or("border-base-200 focus:border-primary"),
            if icon.is_some() { "pl-7" } else { "pl-0" }
        )
    });
    let icon = props.icon.clone();
    let label_style = use_memo(move || {
        to_owned![icon];
        format!(
            "{} {}",
            error_message()
                .and(Some(FLOATING_LABEL_INVALID_STYLE))
                .unwrap_or_default(),
            icon.is_some()
                .then_some("peer-placeholder-shown:pl-7")
                .unwrap_or_default()
        )
    });

    let input_type = props.r#type.clone().unwrap_or("text".to_string());
    let mut validate = use_signal(|| false);
    let _ = use_memo(move || {
        if props.force_validation.unwrap_or_default() || validate() {
            validate_value::<T>(&(*props.value)(), error_message, required);
        }
    });

    let _ = use_resource(move || async move {
        if props.autofocus.unwrap_or_default() {
            let name = (props.name)();
            if let Err(error) = focus_and_select_input_element(&name).await {
                error!("Error focusing element task-project-input: {error:?}");
            }
        }
    });

    rsx! {
        div {
            class: "relative w-full",

            if let Some(icon) = &props.icon {
                div {
                    class: "absolute inset-y-0 start-0 flex py-2.5 pointer-events-none {label_style}",
                    { icon }
                }
            }

            input {
                "type": "{input_type}",
                name: "{props.name}",
                id: "{props.name}",
                class: "{input_style} block py-2 px-3 w-full bg-transparent border-0 border-b-2 focus:outline-none focus:ring-0 peer",
                placeholder: " ",
                required: "{required}",
                value: "{props.value}",
                oninput: move |evt| {
                    props.value.write().clone_from(&evt.value());
                },
                onchange: move |evt| {
                    props.value.write().clone_from(&evt.value());
                },
                onfocusout: move |_| *validate.write() = true,
                autofocus: props.autofocus.unwrap_or_default(),
            }

            if let Some(label) = (props.label)() {
                label {
                    "for": "{props.name}",
                    class: "{label_style} {required_label_style} absolute duration-300 transform -translate-y-6 scale-75 top-3 origin-[0] peer-placeholder-shown:scale-100 peer-placeholder-shown:translate-y-0 peer-focus:scale-75 peer-focus:-translate-y-6 peer-focus:left-0 peer-focus:pl-0",
                    "{label}"
                }
            }

            ErrorMessage { message: error_message }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct InputSelectProps<T: Clone + PartialEq + 'static> {
    name: ReadOnlySignal<String>,
    #[props(!optional)]
    label: ReadOnlySignal<Option<String>>,
    #[props(default)]
    class: Option<String>,
    #[props(default)]
    required: Option<bool>,
    #[props(default)]
    autofocus: Option<bool>,
    #[props(default)]
    force_validation: Option<bool>,
    on_select: Option<EventHandler<Option<T>>>,
    children: Element,
    #[props(default)]
    phantom: PhantomData<T>,
}

#[component]
pub fn FloatingLabelSelect<T>(props: InputSelectProps<T>) -> Element
where
    T: FromStr + Display + Clone + PartialEq,
    <T as FromStr>::Err: Display,
{
    let required = props.required.unwrap_or_default();
    let required_label_style = required
        .then_some("after:content-['*'] after:ml-0.5 after:text-error")
        .unwrap_or_default();

    let error_message = use_signal(|| None);
    let input_style = use_memo(move || {
        error_message()
            .and(Some(INPUT_INVALID_STYLE))
            .unwrap_or("border-base-200 focus:border-primary")
    });
    let label_style = use_memo(move || {
        error_message()
            .and(Some(FLOATING_LABEL_INVALID_STYLE))
            .unwrap_or_default()
    });
    let mut value_to_validate = use_signal(|| "".to_string());
    let mut validate = use_signal(|| false);
    let _ = use_memo(move || {
        if props.force_validation.unwrap_or_default() || (*validate)() {
            validate_value::<T>(&value_to_validate(), error_message, required);
        }
    });

    rsx! {
        div {
            class: "relative {props.class.unwrap_or_default()}",
            select {
                id: "{props.name}",
                name: "{props.name}",
                class: "{input_style} block py-2 px-3 w-full bg-transparent bg-right border-0 border-b-2 appearance-none focus:outline-none focus:ring-0 peer",
                oninput: move |evt| {
                    *validate.write() = true;
                    value_to_validate.write().clone_from(&evt.data.value());
                    if let Some(on_select) = &props.on_select {
                        on_select.call(T::from_str(&evt.data.value()).ok());
                    }
                },
                onfocusout: move |_| *validate.write() = true,
                autofocus: props.autofocus.unwrap_or_default(),

                if !required {
                    option { "" }
                }
                { props.children }
            }

            if let Some(label) = (props.label)() {
                label {
                    "for": "{props.name}",
                    class: "{label_style} {required_label_style} absolute duration-300 transform -translate-y-6 scale-75 top-3 -z-10 origin-[0]",
                    "{label}"
                }
            }

            ErrorMessage { message: error_message }
        }
    }
}

pub trait Searchable {
    // Returns a string to be used to render the searchable object
    fn get_title(&self) -> String;
    // Returns a unique identifier of the searchable object
    fn get_id(&self) -> String;
}

#[derive(Props, Clone, PartialEq)]
pub struct InputSearchProps<T: Clone + PartialEq + Searchable + 'static>
where
    T: Clone + PartialEq + Searchable + 'static,
{
    name: ReadOnlySignal<String>,
    #[props(!optional)]
    label: ReadOnlySignal<Option<String>>,
    value: Signal<Option<T>>,
    search_expression: Signal<String>,
    search_results: Signal<Vec<T>>,
    #[props(default)]
    required: Option<bool>,
    #[props(default)]
    autofocus: Option<bool>,
    on_select: EventHandler<T>,
    #[props(default)]
    class: Option<String>,
    children: Element,
}

#[component]
pub fn FloatingLabelInputSearchSelect<T>(mut props: InputSearchProps<T>) -> Element
where
    T: Clone + PartialEq + Searchable + 'static,
{
    let mut dropdown_opened = use_signal(|| false);
    let required = props.required.unwrap_or_default();
    let required_label_style = required
        .then_some("after:content-['*'] after:ml-0.5 after:text-error")
        .unwrap_or_default();

    let mut selected_index = use_signal(|| 0);
    let _ = use_memo(move || *selected_index.write() = 0);

    let selected_result_title = use_memo(move || {
        (*props.value)()
            .map(|value| value.get_title())
            .unwrap_or_default()
    });

    let mut error_message = use_signal(|| None);
    let button_style = use_memo(move || {
        if (*props.value)().is_none() {
            if is_dropdown_opened(
                (*props.value)().is_some(),
                props.autofocus.unwrap_or_default(),
                dropdown_opened(),
            ) {
                "issearching"
            } else {
                "isempty text-opacity-0 text-base-100"
            }
        } else {
            "isnotempty"
        }
    });
    let border_style = use_memo(move || {
        error_message()
            .and(Some(INPUT_INVALID_STYLE))
            .unwrap_or("border-base-200 focus:border-primary")
    });
    let label_style = use_memo(move || {
        error_message()
            .and(Some(FLOATING_LABEL_INVALID_STYLE))
            .unwrap_or_default()
    });

    let dropdown_style = use_memo(move || {
        if is_dropdown_opened(
            (*props.value)().is_some(),
            props.autofocus.unwrap_or_default(),
            dropdown_opened(),
        ) {
            "visible opacity-100"
        } else {
            "invisible opacity-0"
        }
    });

    let mut button_just_got_focus = use_signal(|| false);
    let _ = use_resource(move || async move {
        if props.autofocus.unwrap_or_default() || dropdown_opened() {
            let name = (props.name)();
            if let Err(error) = focus_and_select_input_element(&name).await {
                error!("Error focusing element {}: {error:?}", name);
            }
        };
    });

    // Tricks to be able to prevent default Enter behavior as Dioxus does not yet support
    // preventing an event conditionnaly (ie. in a handler).
    // This creates a `keydown` event handler using the DOM API on the `search-list` and set the
    // `select_value` flag to trigger the following `use_memo`.
    let mut select_value = use_signal(|| false);
    let _ = use_resource(move || async move {
        let Ok(search_list) = wait_for_element_by_id("search-list", 300).await else {
            error!("Element `search-list` not found");
            return;
        };
        let handler = Closure::wrap(Box::new(move |evt: KeyboardEvent| {
            if &evt.key() == "Enter" {
                *select_value.write() = true;
                evt.prevent_default();
            }
        }) as Box<dyn FnMut(KeyboardEvent)>);

        search_list
            .add_event_listener_with_callback("keydown", handler.as_ref().unchecked_ref())
            .expect("Failed to add `keydown` event listener to search-list");
        handler.forget();
    });

    use_effect(move || {
        if select_value() {
            *select_value.write() = false;
            let result = &(*props.search_results)()[(*selected_index)()];
            *error_message.write() = None;
            *props.value.write() = Some(result.clone());
            *props.search_expression.write() = "".to_string();
            *props.search_results.write() = vec![];
            *dropdown_opened.write() = false;
            props.on_select.call(result.clone());
        }
    });

    rsx! {
        div {
            class: "dropdown group {props.class.unwrap_or_default()}",

            label {
                class: "join h-10 group w-full",
                tabindex: -1,

                { props.children }

                button {
                    id: "selected-result",
                    name: "selected-result",
                    "type": "button",
                    class: "{border_style} {button_style} grow truncate block bg-transparent text-left border-0 border-b-2 focus:outline-none focus:ring-0 peer join-item px-3",
                    onclick: move |_| {
                        if button_just_got_focus() {
                            // Focus has already opened the dropdown, no need to handle click
                            // and close it
                            *button_just_got_focus.write() = false;
                            return;
                        }
                        *dropdown_opened.write() = !dropdown_opened();
                    },
                    onfocus: move |_| {
                        *button_just_got_focus.write() = true;
                        *dropdown_opened.write() = !dropdown_opened();
                    },

                    "{selected_result_title}"
                }
                span {
                    class: "{border_style} block py-2 bg-transparent border-0 border-b-2 join-item",
                    ArrowDown { class: "h-5 w-5 group-hover:visible invisible" }
                }

                if let Some(label) = (props.label)() {
                    label {
                        "for": "selected-result",
                        class: "{label_style} {required_label_style} absolute duration-300 transform -translate-y-0 scale-100 top-2 z-10 origin-[0] peer-[.isnotempty]:scale-75 peer-[.isnotempty]:-translate-y-6 peer-[.issearching]:scale-75 peer-[.issearching]:-translate-y-6 peer-focus:scale-75 peer-focus:-translate-y-6",
                        "{label}"
                    }
                }
            }

            ErrorMessage { message: error_message }

            div {
                class: "{dropdown_style} rounded-box absolute z-50 group-focus:visible group-focus:opacity-100 w-full my-2 shadow-sm menu bg-base-200 overflow-y-scroll max-h-64",
                ul {
                    id: "search-list",
                    tabindex: -1,
                    class: "divide-y",
                    onkeydown: move |evt| {
                        match evt.key() {
                            Key::ArrowDown => {
                                let value = selected_index();
                                if value < (props.search_results)().len() - 1 {
                                    *selected_index.write() = value + 1;
                                }
                            }
                            Key::ArrowUp => {
                                let value = selected_index();
                                if value > 0 {
                                    *selected_index.write() = value - 1;
                                }
                            }
                            Key::Tab | Key::Escape => {
                                if required {
                                    *error_message.write() = Some(format!(
                                        "{} value required",
                                        (props.label)().unwrap_or_default()
                                    ));
                                }
                            }
                            _ => {}
                        }
                    },

                    li {
                        class: "w-full bg-base-400",

                        input {
                            "type": "text",
                            name: "{props.name}",
                            id: "{props.name}",
                            class: "input bg-transparent w-full pl-12 focus:ring-0 focus:outline-none h-10",
                            placeholder: " ",
                            value: "{props.search_expression}",
                            autocomplete: "off",
                            oninput: move |evt| {
                                props.search_expression.write().clone_from(&evt.value());
                            },
                            autofocus: props.autofocus.unwrap_or_default(),
                        }
                        Icon {
                            class: "p-0 w-6 h-6 absolute my-2 ml-2 opacity-60 text-base-content",
                            icon: BsSearch
                        }
                    }

                    {
                        (props.search_results)().into_iter().enumerate().map(|(i, result)| {
                            rsx! {
                                SearchResultRow {
                                    key: "{result.get_id()}",
                                    title: "{result.get_title()}",
                                    selected: i == selected_index(),
                                    on_select: move |_| {
                                        *error_message.write() = None;
                                        *props.value.write() = Some(result.clone());
                                        *props.search_expression.write() = "".to_string();
                                        *props.search_results.write() = vec![];
                                        *dropdown_opened.write() = false;
                                        props.on_select.call(result.clone());
                                    },
                                }
                            }
                        })
                    }
                }
            }
        }
    }
}

#[component]
fn ArrowDown(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: "{class.unwrap_or_default()}",
            role: "img",
            "viewBox": "0 0 24 24",
            fill: "currentColor",
            "fill-rule": "evenodd",
            stroke: "currentColor",
            "stroke-linecap": "round",
            "stroke-linejoin": "round",
            title { "arrow down" }
            path {
                d: "M16 10l-4 4-4-4"
            }
        }
    }
}

fn is_dropdown_opened(has_value: bool, autofocus: bool, dropdown_opened: bool) -> bool {
    (!has_value && autofocus) || dropdown_opened
}

#[component]
pub fn ErrorMessage(message: ReadOnlySignal<Option<String>>) -> Element {
    if let Some(error) = message() {
        rsx! {
            p {
                class: "mt-2 text-error dark:text-error",
                span { "{error}" }
            }
        }
    } else {
        None
    }
}

#[component]
fn SearchResultRow(
    selected: bool,
    title: ReadOnlySignal<String>,
    on_select: EventHandler<()>,
) -> Element {
    let style = if selected { "active" } else { "" };

    rsx! {
        li {
            class: "w-full inline-block",
            prevent_default: "onclick",
            onclick: move |_| {
                on_select.call(());
            },

            span {
                class: "w-full {style} block",

                p { class: "truncate", "{title}" }
            }
        }
    }
}

fn validate_value<T>(value: &str, mut error_message: Signal<Option<String>>, required: bool)
where
    T: FromStr,
    <T as FromStr>::Err: Display,
{
    if value.is_empty() {
        if required {
            let msg = if let Err(error) = T::from_str(value) {
                error.to_string()
            } else {
                "Value required".to_string()
            };
            *error_message.write() = Some(msg);
        } else {
            *error_message.write() = None;
        }
    } else {
        *error_message.write() = T::from_str(value).err().map(|error| error.to_string());
    }
}

#[cfg(test)]
mod tests {
    mod compute_dropdown_style {
        use super::super::*;
        use wasm_bindgen_test::*;

        #[wasm_bindgen_test]
        fn test_has_no_value_and_autofocus() {
            assert!(is_dropdown_opened(false, true, false));
        }

        #[wasm_bindgen_test]
        fn test_has_no_value_and_not_autofocus() {
            assert!(!is_dropdown_opened(false, false, false));
        }

        #[wasm_bindgen_test]
        fn test_has_no_value_and_not_autofocus_and_opened() {
            assert!(is_dropdown_opened(false, false, true));
        }

        #[wasm_bindgen_test]
        fn test_has_value_and_opened() {
            assert!(is_dropdown_opened(true, false, true));
        }

        #[wasm_bindgen_test]
        fn test_has_value_and_not_opened() {
            assert!(!is_dropdown_opened(true, false, false));
        }
    }
}
