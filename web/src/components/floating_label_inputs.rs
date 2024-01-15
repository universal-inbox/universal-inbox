#![allow(non_snake_case)]

use std::{fmt::Display, marker::PhantomData, str::FromStr};

use dioxus::{html::input_data::keyboard_types::Key, prelude::*};
use dioxus_free_icons::{icons::bs_icons::BsSearch, Icon};
use log::error;
use wasm_bindgen::{prelude::Closure, JsCast};
use web_sys::KeyboardEvent;

use crate::utils::{focus_and_select_input_element, get_element_by_id};

#[derive(Props)]
pub struct InputProps<'a, T: 'static> {
    name: String,
    label: String,
    required: bool,
    value: UseState<String>,
    #[props(default)]
    autofocus: Option<bool>,
    #[props(default)]
    force_validation: Option<bool>,
    #[props(default)]
    r#type: Option<String>,
    #[props(default)]
    icon: Option<Element<'a>>,
    #[props(default)]
    phantom: PhantomData<T>,
}

const INPUT_INVALID_STYLE: &str = "border-error focus:border-error";
const FLOATING_LABEL_INVALID_STYLE: &str = "text-error peer-focus:text-error";

pub fn FloatingLabelInputText<'a, T>(cx: Scope<'a, InputProps<'a, T>>) -> Element
where
    T: FromStr,
    <T as FromStr>::Err: Display,
{
    let required_label_style = cx
        .props
        .required
        .then_some("after:content-['*'] after:ml-0.5 after:text-error")
        .unwrap_or_default();

    let error_message = use_state(cx, || None);
    let input_style = use_memo(
        cx,
        &(error_message.clone(), cx.props.icon.is_some()),
        |(error_message, has_icon)| {
            format!(
                "{} {}",
                error_message
                    .as_ref()
                    .and(Some(INPUT_INVALID_STYLE))
                    .unwrap_or("border-base-200 focus:border-primary"),
                if has_icon { "pl-7" } else { "pl-0" }
            )
        },
    );
    let label_style = use_memo(
        cx,
        &(error_message.clone(), cx.props.icon.is_some()),
        |(error_message, has_icon)| {
            format!(
                "{} {}",
                error_message
                    .as_ref()
                    .and(Some(FLOATING_LABEL_INVALID_STYLE))
                    .unwrap_or_default(),
                has_icon
                    .then_some("peer-placeholder-shown:pl-7")
                    .unwrap_or_default()
            )
        },
    );

    let input_type = cx.props.r#type.clone().unwrap_or("text".to_string());
    let validate = use_state(cx, || false);
    let _ = use_memo(
        cx,
        &(
            cx.props.value.clone(),
            cx.props.force_validation,
            validate.clone(),
        ),
        |(value, force_validation, validate)| {
            to_owned![error_message];
            if force_validation.unwrap_or_default() || *validate {
                validate_value::<T>(&value, error_message, cx.props.required);
            }
        },
    );

    use_future(
        cx,
        &(cx.props.autofocus, cx.props.name.clone()),
        |(autofocus, id)| async move {
            if autofocus.unwrap_or_default() {
                if let Err(error) = focus_and_select_input_element(&id).await {
                    error!("Error focusing element task-project-input: {error:?}");
                }
            }
        },
    );

    render! {
        div {
            class: "relative w-full",

            if let Some(icon) = &cx.props.icon {
                render! {
                    div {
                        class: "absolute inset-y-0 start-0 flex py-2.5 pointer-events-none {label_style}",
                        icon
                    }
                }
            }

            input {
                "type": "{input_type}",
                name: "{cx.props.name}",
                id: "{cx.props.name}",
                class: "{input_style} block py-2 px-0 w-full bg-transparent border-0 border-b-2 focus:outline-none focus:ring-0 peer",
                placeholder: " ",
                required: "{cx.props.required}",
                value: "{cx.props.value}",
                oninput: move |evt| {
                    cx.props.value.set(evt.value.clone());
                },
                onchange: move |evt| {
                    cx.props.value.set(evt.value.clone());
                },
                onfocusout: |_| validate.set(true),
                autofocus: cx.props.autofocus.unwrap_or_default(),
            }
            label {
                "for": "{cx.props.name}",
                class: "{label_style} {required_label_style} absolute duration-300 transform -translate-y-6 scale-75 top-3 origin-[0] peer-placeholder-shown:scale-100 peer-placeholder-shown:translate-y-0 peer-focus:scale-75 peer-focus:-translate-y-6 peer-focus:left-0 peer-focus:pl-0",
                "{cx.props.label}"
            }
            ErrorMessage { message: error_message }
        }
    }
}

pub fn FloatingLabelSelect<'a, T>(cx: Scope<'a, InputProps<'a, T>>) -> Element
where
    T: FromStr,
    <T as FromStr>::Err: Display,
{
    let required_label_style = cx
        .props
        .required
        .then_some("after:content-['*'] after:ml-0.5 after:text-error")
        .unwrap_or_default();

    let error_message = use_state(cx, || None);
    let input_style = use_memo(cx, &(error_message.clone(),), |(error_message,)| {
        error_message
            .as_ref()
            .and(Some(INPUT_INVALID_STYLE))
            .unwrap_or("border-base-200 focus:border-primary")
    });
    let label_style = use_memo(cx, &(error_message.clone(),), |(error_message,)| {
        error_message
            .as_ref()
            .and(Some(FLOATING_LABEL_INVALID_STYLE))
            .unwrap_or_default()
    });

    let validate = use_state(cx, || false);
    let _ = use_memo(
        cx,
        &(
            cx.props.value.clone(),
            cx.props.force_validation,
            validate.clone(),
        ),
        |(value, force_validation, validate)| {
            to_owned![error_message];
            if force_validation.unwrap_or_default() || *validate {
                validate_value::<T>(&value, error_message, cx.props.required);
            }
        },
    );

    render! {
        div {
            class: "relative",
            select {
                id: "{cx.props.name}",
                name: "{cx.props.name}",
                class: "{input_style} block py-2 px-0 w-full bg-transparent bg-right border-0 border-b-2 appearance-none focus:outline-none focus:ring-0 peer",
                oninput: move |evt| {
                    validate.set(true);
                    cx.props.value.set(evt.data.value.clone());
                },
                onfocusout: |_| validate.set(true),
                value: "{cx.props.value}",
                autofocus: cx.props.autofocus.unwrap_or_default(),
                option { value: "1", "🔴 Priority 1" }
                option { value: "2", "🟠 Priority 2" }
                option { value: "3", "🟡 Priority 3" }
                option { value: "4", "🔵 Priority 4" }
            }
            label {
                "for": "{cx.props.name}",
                class: "{label_style} {required_label_style} absolute duration-300 transform -translate-y-6 scale-75 top-3 -z-10 origin-[0]",
                "{cx.props.label}"
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

#[derive(Props)]
pub struct InputSearchProps<'a, T>
where
    T: Clone + Searchable + 'static,
{
    name: String,
    label: String,
    value: UseState<Option<T>>,
    search_expression: UseState<String>,
    search_results: UseState<Vec<T>>,
    #[props(default)]
    required: Option<bool>,
    #[props(default)]
    autofocus: Option<bool>,
    on_select: EventHandler<'a, T>,
    children: Element<'a>,
}

pub fn FloatingLabelInputSearchSelect<'a, T>(cx: Scope<'a, InputSearchProps<'a, T>>) -> Element<'a>
where
    T: Clone + Searchable + 'static,
{
    let dropdown_opened = use_state(cx, || false);
    let required_label_style = cx
        .props
        .required
        .unwrap_or_default()
        .then_some("after:content-['*'] after:ml-0.5 after:text-error")
        .unwrap_or_default();

    let selected_index = use_state(cx, || 0);
    let _ = use_memo(cx, (&cx.props.search_results,), |(_,)| {
        selected_index.set(0)
    });

    let selected_result_title = use_memo(cx, &cx.props.value.clone(), |value| {
        (*value.current())
            .as_ref()
            .map(|value| value.get_title())
            .unwrap_or_default()
    });

    let error_message = use_state(cx, || None);
    let button_style = use_memo(
        cx,
        &(
            cx.props.value.clone(),
            cx.props.autofocus,
            dropdown_opened.clone(),
        ),
        |(value, autofocus, dropdown_opened)| {
            if value.is_none() {
                if is_dropdown_opened(
                    value.current().is_some(),
                    autofocus.unwrap_or_default(),
                    *dropdown_opened.current(),
                ) {
                    "issearching"
                } else {
                    "isempty text-opacity-0 text-base-100"
                }
            } else {
                "isnotempty"
            }
        },
    );
    let border_style = use_memo(cx, &(error_message.clone(),), |(error_message,)| {
        error_message
            .as_ref()
            .and(Some(INPUT_INVALID_STYLE))
            .unwrap_or("border-base-200 focus:border-primary")
    });
    let label_style = use_memo(cx, &(error_message.clone(),), |(error_message,)| {
        error_message
            .as_ref()
            .and(Some(FLOATING_LABEL_INVALID_STYLE))
            .unwrap_or_default()
    });

    let dropdown_style = use_memo(
        cx,
        &(
            cx.props.value.clone(),
            cx.props.autofocus,
            dropdown_opened.clone(),
        ),
        |(value, autofocus, dropdown_opened)| {
            if is_dropdown_opened(
                value.current().is_some(),
                autofocus.unwrap_or_default(),
                *dropdown_opened.current(),
            ) {
                "visible opacity-100"
            } else {
                "invisible opacity-0"
            }
        },
    );

    use_future(
        cx,
        &(cx.props.autofocus, cx.props.name.clone()),
        |(autofocus, id)| async move {
            if autofocus.unwrap_or_default() {
                if let Err(error) = focus_and_select_input_element(&id).await {
                    error!("Error focusing element {id}: {error:?}");
                }
            }
        },
    );

    let select_result = |result: Option<T>| {
        dropdown_opened.set(false);

        match result {
            Some(result) => {
                error_message.set(None);
                cx.props.value.set(Some(result.clone()));
                cx.props.search_expression.set("".to_string());
                cx.props.search_results.set(vec![]);
                cx.props.on_select.call(result);
            }
            None => {
                error_message.set(Some(format!("{} value required", cx.props.label)));
            }
        }
    };

    // Tricks to be able to prevent default Enter behavior as Dioxus does not yet support
    // preventing an event conditionnaly (ie. in a handler).
    // This creates a `keydown` event handler using the DOM API on the `search-list` and set the
    // `select_value` flag to trigger the following `use_memo`.
    // Indeed, it is not possible to do call `select_result` from the DOM event handler as it has a
    // 'static lifetime where as `select_result` has a 'a lifetime.
    let select_value = use_state(cx, || false);
    use_future(cx, (), |()| {
        to_owned![select_value];

        async move {
            let handler = Closure::wrap(Box::new(move |evt: KeyboardEvent| {
                if &evt.key() == "Enter" {
                    select_value.set(true);
                    evt.prevent_default();
                }
            }) as Box<dyn FnMut(KeyboardEvent)>);

            let search_list = get_element_by_id("search-list").unwrap();
            search_list
                .add_event_listener_with_callback("keydown", handler.as_ref().unchecked_ref())
                .expect("Failed to add `keydown` event listener to search-list");
            handler.forget();
        }
    });

    let _ = use_memo(
        cx,
        &(select_value.clone(), cx.props.search_results.clone()),
        |(select_value, search_results)| {
            if *select_value.current() {
                select_value.set(false);
                let result = &search_results[*selected_index.current()];
                select_result(Some(result.clone()));
            }
        },
    );

    render! {
        div {
            class: "dropdown bg-base-100 group",

            label {
                class: "join h-10 group w-full",
                tabindex: -1,

                &cx.props.children

                button {
                    id: "selected-result",
                    name: "selected-result",
                    class: "{border_style} {button_style} grow truncate block bg-transparent text-left border-0 border-b-2 focus:outline-none focus:ring-0 peer join-item",
                    onfocus: |_| {
                        let value = !*dropdown_opened.current();
                        let id = cx.props.name.clone();
                        dropdown_opened.set(value);
                        if value {
                            cx.spawn(async move {
                                if let Err(error) = focus_and_select_input_element(&id).await {
                                    error!("Error focusing element {id}: {error:?}");
                                }
                            });
                        }
                    },

                    "{selected_result_title}"
                }
                span {
                    class: "{border_style} block py-2 bg-transparent border-0 border-b-2 join-item",
                    ArrowDown { class: "h-5 w-5 group-hover:visible invisible" }
                }
                label {
                    "for": "selected-result",
                    class: "{label_style} {required_label_style} absolute duration-300 transform -translate-y-0 scale-100 top-2 z-10 origin-[0] peer-[.isnotempty]:scale-75 peer-[.isnotempty]:-translate-y-6 peer-[.issearching]:scale-75 peer-[.issearching]:-translate-y-6 peer-focus:scale-75 peer-focus:-translate-y-6",
                    "{cx.props.label}"
                }
            }
            ErrorMessage { message: error_message }

            ul {
                id: "search-list",
                tabindex: -1,
                class: "{dropdown_style} group-focus:visible group-focus:opacity-100 w-full my-2 shadow-sm menu bg-base-200 divide-y rounded-box absolute z-50 ",
                onkeydown: move |evt| {
                    match evt.key() {
                        Key::ArrowDown => {
                            let value = *selected_index.current();
                            if value < cx.props.search_results.len() - 1 {
                                selected_index.set(value + 1);
                            }
                        }
                        Key::ArrowUp => {
                            let value = *selected_index.current();
                            if value > 0 {
                                selected_index.set(value - 1);
                            }
                        }
                        Key::Tab => {
                            select_result(None);
                        }
                        _ => {}
                    }
                },

                li {
                    class: "w-full bg-base-400",

                    input {
                        "type": "text",
                        name: "{cx.props.name}",
                        id: "{cx.props.name}",
                        class: "input bg-transparent w-full pl-12 focus:ring-0 focus:outline-none h-10",
                        placeholder: " ",
                        value: "{cx.props.search_expression}",
                        autocomplete: "off",
                        oninput: |evt| {
                            cx.props.search_expression.set(evt.value.clone());
                        },
                        autofocus: cx.props.autofocus.unwrap_or_default(),
                    }
                    Icon {
                        class: "p-0 w-6 h-6 absolute my-2 ml-2 opacity-60 text-base-content",
                        icon: BsSearch
                    }

                }

                cx.props.search_results.iter().enumerate().map(|(i, result)| {
                    render! {
                        SearchResultRow {
                            key: "{result.get_id()}",
                            title: "{result.get_title()}",
                            selected: i == *selected_index.current(),
                            on_select: move |_| { select_result(Some(result.clone())); },
                        }
                    }
                })
            }
        }
    }
}

#[component]
fn ArrowDown<'a>(cx: Scope, class: Option<&'a str>) -> Element {
    render! {
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
pub fn ErrorMessage<'a>(cx: Scope, message: &'a UseState<Option<String>>) -> Element {
    if let Some(error) = message.as_ref() {
        render! {
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
fn SearchResultRow<'a>(
    cx: Scope,
    selected: bool,
    title: &'a str,
    on_select: EventHandler<'a, ()>,
) -> Element {
    let style = use_memo(
        cx,
        (selected,),
        |(selected,)| {
            if selected {
                "active"
            } else {
                ""
            }
        },
    );

    render! {
        li {
            class: "w-full inline-block",
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

fn validate_value<T>(value: &str, error_message: UseState<Option<String>>, required: bool)
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
            error_message.set(Some(msg));
        } else {
            error_message.set(None);
        }
    } else {
        error_message.set(T::from_str(value).err().map(|error| error.to_string()));
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
