use log::error;
use std::{fmt::Display, marker::PhantomData, str::FromStr};

use dioxus::prelude::*;

use crate::utils::focus_and_select_element;

#[derive(PartialEq, Props)]
pub struct InputProps<T: 'static> {
    name: String,
    label: String,
    required: bool,
    value: UseState<String>,
    #[props(default)]
    autofocus: Option<bool>,
    #[props(default)]
    force_validation: Option<bool>,
    #[props(default)]
    phantom: PhantomData<T>,
}

const INPUT_VALID_STYLE: &str = "input-valid";
const INPUT_INVALID_STYLE: &str = "input-invalid";
const FLOATING_LABEL_VALID_STYLE: &str = "floating-label-valid";
const FLOATING_LABEL_INVALID_STYLE: &str = "floating-label-invalid";

pub fn floating_label_input_text<T>(cx: Scope<InputProps<T>>) -> Element
where
    T: FromStr,
    <T as FromStr>::Err: Display,
{
    let required_label_style = cx
        .props
        .required
        .then_some("after:content-['*'] after:ml-0.5 after:text-red-500")
        .unwrap_or_default();

    let error_message = use_state(cx, || None);
    let input_style = use_memo(cx, &(error_message.clone(),), |(error_message,)| {
        if error_message.current().is_some() {
            INPUT_INVALID_STYLE.to_string()
        } else {
            INPUT_VALID_STYLE.to_string()
        }
    });
    let label_style = use_memo(cx, &(error_message.clone(),), |(error_message,)| {
        if error_message.current().is_some() {
            FLOATING_LABEL_INVALID_STYLE.to_string()
        } else {
            FLOATING_LABEL_VALID_STYLE.to_string()
        }
    });

    let validate = use_state(cx, || false);
    use_memo(
        cx,
        &(
            cx.props.value.clone(),
            cx.props.force_validation,
            validate.clone(),
        ),
        |(value, force_validation, validate)| {
            to_owned![error_message];
            if force_validation.unwrap_or_default() || *validate {
                validate_value::<T>(&value, error_message);
            }
        },
    );

    use_future(
        cx,
        &(cx.props.autofocus, cx.props.name.clone()),
        |(autofocus, id)| async move {
            if autofocus.unwrap_or_default() {
                if let Err(error) = focus_and_select_element(&id).await {
                    error!("Error focusing element task-project-input: {error:?}");
                }
            }
        },
    );

    cx.render(rsx!(
        div {
            class: "relative z-0 grow",
            input {
                "type": "text",
                name: "{cx.props.name}",
                id: "{cx.props.name}",
                class: "{input_style} block py-2.5 px-0 w-full text-sm bg-transparent border-0 border-b-2 focus:outline-none focus:ring-0 peer",
                placeholder: " ",
                required: "{cx.props.required}",
                value: "{cx.props.value}",
                oninput: move |evt| {
                    validate.set(true);
                    cx.props.value.set(evt.value.clone());
                },
                onfocusout: |_| validate.set(true),
                autofocus: cx.props.autofocus.unwrap_or_default(),
            }
            label {
                "for": "{cx.props.name}",
                class: "{label_style} {required_label_style} absolute text-sm duration-300 transform -translate-y-6 scale-75 top-3 -z-10 origin-[0] peer-focus:left-0 peer-placeholder-shown:scale-100 peer-placeholder-shown:translate-y-0 peer-focus:scale-75 peer-focus:-translate-y-6",
                "{cx.props.label}"
            }
            self::error_message { message: error_message }
        }
    ))
}

pub fn floating_label_input_date<T>(cx: Scope<InputProps<T>>) -> Element
where
    T: FromStr,
    <T as FromStr>::Err: Display,
{
    const IS_NOT_EMPTY_STYLE: &str = "isnotempty";
    const IS_EMPTY_STYLE: &str = "isempty text-opacity-0";

    let required_label_style = cx
        .props
        .required
        .then_some("after:content-['*'] after:ml-0.5 after:text-red-500")
        .unwrap_or_default();

    let input_empty_style = use_memo(cx, &(cx.props.value.clone(),), |(value,)| {
        if value.is_empty() {
            format!("{} {}", IS_EMPTY_STYLE, INPUT_VALID_STYLE)
        } else {
            format!("{} {}", IS_NOT_EMPTY_STYLE, INPUT_VALID_STYLE)
        }
    });

    let error_message = use_state(cx, || None);
    let label_style = use_memo(cx, &error_message.clone(), |error_message| {
        if error_message.current().is_some() {
            FLOATING_LABEL_INVALID_STYLE.to_string()
        } else {
            FLOATING_LABEL_VALID_STYLE.to_string()
        }
    });
    let input_style = use_memo(
        cx,
        &(error_message.clone(), input_empty_style.clone()),
        |(error_message, input_style)| {
            if error_message.current().is_some() {
                input_style.replace(INPUT_VALID_STYLE, INPUT_INVALID_STYLE)
            } else {
                input_style.replace(INPUT_INVALID_STYLE, INPUT_VALID_STYLE)
            }
        },
    );

    let validate = use_state(cx, || false);
    use_memo(
        cx,
        &(
            cx.props.value.clone(),
            cx.props.force_validation,
            validate.clone(),
        ),
        |(value, force_validation, validate)| {
            to_owned![error_message];
            if force_validation.unwrap_or_default() || *validate {
                validate_value::<T>(&value, error_message);
            }
        },
    );

    cx.render(rsx!(
        div {
            class: "relative z-0 grow",
            input {
                "type": "date",
                name: "{cx.props.name}",
                id: "{cx.props.name}",
                class: "{input_style} block py-2.5 px-0 w-full text-sm bg-transparent border-0 border-b-2 text-white focus:text-opacity-1 focus:text-black focus:dark:text-white focus:outline-none focus:ring-0 peer",
                required: "{cx.props.required}",
                value: "{cx.props.value}",
                oninput: move |evt| {
                    validate.set(true);
                    cx.props.value.set(evt.value.clone());
                },
                autofocus: cx.props.autofocus.unwrap_or_default(),
            }
            label {
                "for": "{cx.props.name}",
                class: "{label_style} {required_label_style} absolute text-sm duration-300 transform -translate-y-0 scale-100 top-3 -z-10 origin-[0] peer-focus:left-0 peer-[.isnotempty]:left-0 peer-[.isnotempty]:scale-75 peer-[.isnotempty]:-translate-y-6 peer-focus:scale-75 peer-focus:-translate-y-6",
                "{cx.props.label}"
            }
            self::error_message { message: error_message }
        }
    ))
}

pub fn floating_label_select<T>(cx: Scope<InputProps<T>>) -> Element
where
    T: FromStr,
    <T as FromStr>::Err: Display,
{
    let required_label_style = cx
        .props
        .required
        .then_some("after:content-['*'] after:ml-0.5 after:text-red-500")
        .unwrap_or_default();

    let error_message = use_state(cx, || None);
    let input_style = use_memo(cx, &(error_message.clone(),), |(error_message,)| {
        if error_message.current().is_some() {
            INPUT_INVALID_STYLE.to_string()
        } else {
            INPUT_VALID_STYLE.to_string()
        }
    });
    let label_style = use_memo(cx, &(error_message.clone(),), |(error_message,)| {
        if error_message.current().is_some() {
            FLOATING_LABEL_INVALID_STYLE.to_string()
        } else {
            FLOATING_LABEL_VALID_STYLE.to_string()
        }
    });

    let validate = use_state(cx, || false);
    use_memo(
        cx,
        &(
            cx.props.value.clone(),
            cx.props.force_validation,
            validate.clone(),
        ),
        |(value, force_validation, validate)| {
            to_owned![error_message];
            if force_validation.unwrap_or_default() || *validate {
                validate_value::<T>(&value, error_message);
            }
        },
    );

    cx.render(rsx!(
        div {
            class: "relative z-0 grow",
            select {
                id: "{cx.props.name}",
                name: "{cx.props.name}",
                class: "{input_style} block py-2.5 px-0 w-full text-sm bg-transparent bg-right border-0 border-b-2 appearance-none focus:outline-none focus:ring-0 peer",
                oninput: move |evt| {
                    validate.set(true);
                    cx.props.value.set(evt.data.value.clone());
                },
                onfocusout: |_| validate.set(true),
                value: "{cx.props.value}",
                autofocus: cx.props.autofocus.unwrap_or_default(),
                option { value: "1", "Priority 1" }
                option { value: "2", "Priority 2" }
                option { value: "3", "Priority 3" }
                option { value: "4", "Priority 4" }
            }
            label {
                "for": "{cx.props.name}",
                class: "{label_style} {required_label_style} absolute text-sm duration-300 transform -translate-y-6 scale-75 top-3 -z-10 origin-[0]",
                "{cx.props.label}"
            }
            self::error_message { message: error_message }
        }
    ))
}

#[inline_props]
fn error_message<'a>(cx: Scope, message: &'a UseState<Option<String>>) -> Element {
    message.as_ref().and_then(|error| {
        cx.render(rsx!(
            p {
                class: "mt-2 text-sm text-red-600 dark:text-red-500",
                span { class: "font-medium", "{error}" }
            }
        ))
    })
}

fn validate_value<T>(value: &str, error_message: UseState<Option<String>>)
where
    T: FromStr,
    <T as FromStr>::Err: Display,
{
    error_message.set(T::from_str(value).err().map(|error| error.to_string()));
}
