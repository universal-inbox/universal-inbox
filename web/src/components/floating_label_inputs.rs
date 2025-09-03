#![allow(non_snake_case)]

use std::{fmt::Display, marker::PhantomData, str::FromStr};

use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use json_value_merge::Merge;
use log::error;
use serde_json::json;

use crate::{
    services::flyonui::{
        forget_flyonui_select_element, get_flyonui_selected_remote_value,
        init_flyonui_select_element,
    },
    utils::focus_and_select_input_element,
};

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
    on_update: Option<EventHandler<String>>,
    #[props(default)]
    phantom: PhantomData<T>,
}

#[component]
pub fn FloatingLabelInputText<T>(mut props: InputProps<T>) -> Element
where
    T: FromStr + Clone + PartialEq,
    <T as FromStr>::Err: Display,
{
    let required = props.required.unwrap_or_default();
    let required_label_style = if required {
        "after:content-['*'] after:ml-0.5 after:text-error"
    } else {
        Default::default()
    };
    let has_icon = props.icon.is_some();
    let label_style = use_memo(move || {
        if (props.value)().is_empty() && has_icon {
            "left-8 peer-focus:left-0"
        } else {
            "left-0"
        }
    });

    let error_message = use_signal(|| None);
    let input_style = use_memo(move || error_message().and(Some("is-invalid")).unwrap_or(""));

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
            class: "input-floating input-sm",

            div {
                class: "w-full input rounded-b-none border-t-0 border-l-0 border-r-0 focus-within:outline-none",

                if let Some(icon) = &props.icon {
                    div {
                        class: "text-base-content/80 my-auto me-3 size-5 shrink-0",
                        { icon }
                    }
                }

                input {
                    class: "{input_style} peer",
                    "type": "{input_type}",
                    name: "{props.name}",
                    id: "{props.name}",
                    placeholder: " ",
                    required: "{required}",
                    value: "{props.value}",
                    oninput: move |evt| {
                        props.value.write().clone_from(&evt.value());
                    },
                    onchange: move |evt| {
                        props.value.write().clone_from(&evt.value());
                    },
                    onfocusout: move |_| {
                        *validate.write() = true;
                        if let Some(on_update) = &props.on_update {
                            on_update.call(props.value.read().clone());
                        }
                    },
                    autofocus: props.autofocus.unwrap_or_default(),
                }

                if let Some(label) = (props.label)() {
                    label {
                        "for": "{props.name}",
                        class: "{required_label_style} {label_style} input-floating-label",
                        "{label}"
                    }
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
    #[props(default)]
    default_value: Option<String>,
    #[props(default)]
    disabled: Option<bool>,
    #[props(default)]
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
    let required_label_style = if required {
        "after:content-['*'] after:ml-0.5 after:text-error"
    } else {
        Default::default()
    };

    let error_message = use_signal(|| None);
    let input_style = use_memo(move || error_message().and(Some("is-invalid")).unwrap_or(""));
    let mut value_to_validate = use_signal(|| "".to_string());
    let mut validate = use_signal(|| false);
    use_effect(move || {
        if let Some(default_value) = &props.default_value {
            *value_to_validate.write() = default_value.clone();
        }
    });
    let _ = use_memo(move || {
        if props.force_validation.unwrap_or_default() || (*validate)() {
            validate_value::<T>(&value_to_validate(), error_message, required);
        }
    });
    let class = props.class.unwrap_or_default();

    rsx! {
        div {
            class: "select-floating {class}",
            select {
                id: "{props.name}",
                name: "{props.name}",
                class: "{input_style} select select-sm rounded-b-none border-t-0 border-l-0 border-r-0 focus:outline-none",
                onchange: move |evt| {
                    *validate.write() = true;
                    value_to_validate.write().clone_from(&evt.data.value());
                    if let Some(on_select) = &props.on_select {
                        on_select.call(T::from_str(&evt.data.value()).ok());
                    }
                },
                onfocusout: move |_| *validate.write() = true,
                autofocus: props.autofocus.unwrap_or_default(),
                disabled: props.disabled.unwrap_or_default(),

                if !required {
                    option { "" }
                }
                { props.children }
            }

            if let Some(label) = (props.label)() {
                label {
                    "for": "{props.name}",
                    class: "{required_label_style} select-floating-label",
                    "{label}"
                }
            }

            ErrorMessage { message: error_message }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct InputSearchProps<T: Clone + PartialEq + 'static> {
    name: ReadOnlySignal<String>,
    #[props(optional)]
    label: ReadOnlySignal<Option<String>>,
    #[props(default)]
    required: Option<bool>,
    #[props(default)]
    autofocus: Option<bool>,
    #[props(default)]
    disabled: Option<bool>,
    #[props(default)]
    on_select: Option<EventHandler<Option<T>>>,
    #[props(default)]
    data_select: ReadOnlySignal<Option<serde_json::Value>>,
    #[props(default)]
    class: Option<String>,
    children: Element,
}

#[component]
pub fn FloatingLabelInputSearchSelect<T>(props: InputSearchProps<T>) -> Element
where
    T: Display + Clone + PartialEq + for<'de> serde::Deserialize<'de>,
{
    let required = props.required.unwrap_or_default();
    let required_label_style = if required {
        "after:content-['*'] after:ml-0.5 after:text-error"
    } else {
        Default::default()
    };

    let error_message: Signal<Option<String>> = use_signal(|| None);
    let input_style = use_memo(move || error_message().and(Some("is-invalid")).unwrap_or(""));
    let class = props.class.unwrap_or_default();
    let mut mounted_element: Signal<Option<web_sys::Element>> = use_signal(|| None);

    let data_select = use_memo(move || {
        let mut default_data_select = json!({
            "toggleTag": "<button type=\"button\" aria-expanded=\"false\"></button>",
            "toggleClasses": "advance-select-toggle advance-select-sm select-disabled:pointer-events-none select-disabled:opacity-40 rounded-b-none border-t-0 border-l-0 border-r-0 focus:outline-none",
            "hasSearch": true,
            "minSearchLength": 1,
            "dropdownClasses": "advance-select-menu menu-sm max-h-52 pt-0 overflow-y-auto z-80",
            "optionTemplate": "<div class=\"flex justify-between items-center w-full\"><span data-title></span><span class=\"icon-[tabler--check] shrink-0 size-4 text-primary hidden selected:block \"></span></div>",
            "extraMarkup": "<span class=\"icon-[tabler--chevron-down] shrink-0 size-5 text-base-content/75 absolute top-1/2 end-2.5 -translate-y-1/2 \"></span>",
            "optionClasses": "advance-select-option selected:select-active",
            "optionAllowEmptyOption": true
        });
        if let Some(data_select) = (props.data_select)() {
            default_data_select.merge(&data_select);
        }
        default_data_select
    });

    use_drop(move || {
        if let Some(element) = mounted_element() {
            *mounted_element.write() = None;
            forget_flyonui_select_element(&element);
        }
    });

    rsx! {
        div {
            class: "w-full select-floating {class}",
            select {
                id: "{props.name}",
                name: "{props.name}",
                "data-select": "{data_select}",
                onmounted: move |element| {
                    let web_element = element.as_web_event();
                    init_flyonui_select_element(&web_element);
                    mounted_element.set(Some(web_element));
                },
                class: "{input_style} hidden",
                onchange: move |_| {
                    if let Some(element) = mounted_element() {
                        if let Some(on_select) = &props.on_select {
                            if let Ok(selected_remote_value) = serde_wasm_bindgen::from_value::<T>(get_flyonui_selected_remote_value(&element)) {
                                on_select.call(Some(selected_remote_value));
                            }
                        }
                    }
                },
                disabled: props.disabled.unwrap_or_default(),
            }

            if let Some(label) = (props.label)() {
                label {
                    "for": "{props.name}",
                    class: "{required_label_style} select-floating-label",
                    "{label}"
                }
            }

            ErrorMessage { message: error_message }
        }
    }
}

#[component]
pub fn ErrorMessage(message: ReadOnlySignal<Option<String>>) -> Element {
    if let Some(error) = message() {
        rsx! { span { class: "helper-text ps-3", "{error} "} }
    } else {
        rsx! {}
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
