use log::error;
use std::{fmt::Display, marker::PhantomData, str::FromStr};

use dioxus::{html::input_data::keyboard_types::Key, prelude::*};
use dioxus_free_icons::{icons::bs_icons::BsSearch, Icon};

use universal_inbox::task::TaskSummary;

use crate::{components::icons::todoist, utils::focus_and_select_input_element};

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

const INPUT_INVALID_STYLE: &str = "border-error focus:border-error";
const FLOATING_LABEL_INVALID_STYLE: &str = "text-error peer-focus:text-error";

pub fn floating_label_input_text<T>(cx: Scope<InputProps<T>>) -> Element
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
            .unwrap_or_default()
    });
    let label_style = use_memo(cx, &(error_message.clone(),), |(error_message,)| {
        error_message
            .as_ref()
            .and(Some(FLOATING_LABEL_INVALID_STYLE))
            .unwrap_or_default()
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

    cx.render(rsx!(
        div {
            class: "relative",
            input {
                "type": "text",
                name: "{cx.props.name}",
                id: "{cx.props.name}",
                class: "{input_style} block py-2 px-0 w-full bg-transparent border-0 border-b-2 focus:outline-none focus:ring-0 peer",
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
                class: "{label_style} {required_label_style} absolute duration-300 transform -translate-y-6 scale-75 top-3 -z-10 origin-[0] peer-focus:left-0 peer-placeholder-shown:scale-100 peer-placeholder-shown:translate-y-0 peer-focus:scale-75 peer-focus:-translate-y-6",
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
    const IS_EMPTY_STYLE: &str = "isempty text-opacity-0 text-base-100";

    let required_label_style = cx
        .props
        .required
        .then_some("after:content-['*'] after:ml-0.5 after:text-error")
        .unwrap_or_default();

    let input_empty_style = use_memo(cx, &(cx.props.value.clone(),), |(value,)| {
        if value.is_empty() {
            IS_EMPTY_STYLE
        } else {
            IS_NOT_EMPTY_STYLE
        }
    });

    let error_message = use_state(cx, || None);
    let label_style = use_memo(cx, &error_message.clone(), |error_message| {
        error_message
            .as_ref()
            .and(Some(FLOATING_LABEL_INVALID_STYLE))
            .unwrap_or_default()
    });
    let input_style = use_memo(
        cx,
        &(error_message.clone(), input_empty_style.to_string()),
        |(error_message, input_style)| {
            format!(
                "{input_style} {}",
                error_message
                    .as_ref()
                    .and(Some(INPUT_INVALID_STYLE))
                    .unwrap_or_default()
            )
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
                validate_value::<T>(&value, error_message, cx.props.required);
            }
        },
    );

    cx.render(rsx!(
        div {
            class: "relative",
            input {
                "type": "date",
                name: "{cx.props.name}",
                id: "{cx.props.name}",
                class: "{input_style} block py-2 px-0 w-full bg-transparent border-0 border-b-2 focus:text-opacity-1 focus:text-base-content focus:dark:text-base-content outline-none focus:ring-0 peer",
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
                class: "{label_style} {required_label_style} absolute duration-300 transform -translate-y-0 scale-100 top-3 -z-10 origin-[0] peer-focus:left-0 peer-[.isnotempty]:left-0 peer-[.isnotempty]:scale-75 peer-[.isnotempty]:-translate-y-6 peer-focus:scale-75 peer-focus:-translate-y-6",
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
        .then_some("after:content-['*'] after:ml-0.5 after:text-error")
        .unwrap_or_default();

    let error_message = use_state(cx, || None);
    let input_style = use_memo(cx, &(error_message.clone(),), |(error_message,)| {
        error_message
            .as_ref()
            .and(Some(INPUT_INVALID_STYLE))
            .unwrap_or_default()
    });
    let label_style = use_memo(cx, &(error_message.clone(),), |(error_message,)| {
        error_message
            .as_ref()
            .and(Some(FLOATING_LABEL_INVALID_STYLE))
            .unwrap_or_default()
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
                validate_value::<T>(&value, error_message, cx.props.required);
            }
        },
    );

    cx.render(rsx!(
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
                option { value: "1", "Priority 1" }
                option { value: "2", "Priority 2" }
                option { value: "3", "Priority 3" }
                option { value: "4", "Priority 4" }
            }
            label {
                "for": "{cx.props.name}",
                class: "{label_style} {required_label_style} absolute duration-300 transform -translate-y-6 scale-75 top-3 -z-10 origin-[0]",
                "{cx.props.label}"
            }
            self::error_message { message: error_message }
        }
    ))
}

#[derive(Props)]
pub struct InputSearchProps<'a> {
    name: String,
    label: String,
    value: UseState<Option<TaskSummary>>,
    search_expression: UseState<String>,
    search_results: UseState<Vec<TaskSummary>>,
    #[props(default)]
    autofocus: Option<bool>,
    on_select: EventHandler<'a, TaskSummary>,
}

pub fn floating_label_input_search_select<'a>(cx: Scope<'a, InputSearchProps<'a>>) -> Element<'a> {
    let icon = cx.render(rsx!(self::todoist { class: "h-5 w-5" }));
    let dropdown_opened = use_state(cx, || false);

    let selected_task_title = use_memo(cx, &cx.props.value.clone(), |value| {
        (*value.current())
            .as_ref()
            .map(|task| task.title.clone())
            .unwrap_or_default()
    });

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

    cx.render(rsx!(
        div {
            class: "dropdown bg-base-100 group",

            label {
                class: "input-group h-10 group",
                tabindex: -1,

                span {
                    class: "block py-2 bg-transparent border-0 border-b-2",
                    icon
                }
                button {
                    tabindex: 1,
                    id: "associated-task",
                    name: "associated-task",
                    class: "{button_style} truncate block py-2 px-0 w-full bg-transparent border-0 border-b-2 focus:outline-none focus:ring-0 peer",
                    onclick: |_| {
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

                    "{selected_task_title}"
                }
                span {
                    class: "block py-2 bg-transparent border-0 border-b-2",
                    arrow_down { class: "h-5 w-5 group-hover:visible invisible" }
                }
                label {
                    "for": "associated-task",
                    class: "absolute duration-300 transform -translate-y-0 scale-100 top-2 z-10 origin-[0] peer-[.isnotempty]:scale-75 peer-[.isnotempty]:-translate-y-6 peer-[.issearching]:scale-75 peer-[.issearching]:-translate-y-6 peer-focus:scale-75 peer-focus:-translate-y-6",
                    "{cx.props.label}"
                }
            }

            ul {
                tabindex: -1,
                class: "{dropdown_style} group-focus:visible group-focus:opacity-100 w-full my-2 shadow-sm menu bg-base-200 divide-y rounded-box absolute z-50 ",

                li {
                    class: "w-full bg-base-400",

                    input {
                        "type": "text",
                        name: "{cx.props.name}",
                        id: "{cx.props.name}",
                        class: "input bg-transparent w-full pl-12 focus:ring-0 focus:outline-none",
                        placeholder: " ",
                        value: "{cx.props.search_expression}",
                        autocomplete: "off",
                        tabindex: 2,
                        oninput: |evt| {
                            cx.props.search_expression.set(evt.value.clone());
                        },
                        autofocus: cx.props.autofocus.unwrap_or_default(),
                    }
                    Icon {
                        class: "p-0 w-6 h-6 absolute my-3 ml-2 opacity-60 text-base-content",
                        icon: BsSearch
                    }

                }

                cx.props.search_results.iter().enumerate().map(|(i, task)| {
                    let select_task = || {
                        dropdown_opened.set(false);
                        cx.props.value.set(Some(task.clone()));
                        cx.props.search_expression.set("".to_string());
                        cx.props.search_results.set(vec![]);
                        cx.props.on_select.call(task.clone());
                    };

                    rsx!(

                        li {
                            class: "w-full",
                            key: "{task.id}",
                            tabindex: "{i + 3}",
                            onclick: move |_| select_task(),
                            onkeydown: move |evt| {
                                if evt.key() == Key::Enter {
                                    select_task();
                                }
                            },

                            span {
                                class: "w-full",

                                p { class: "truncate", "{task.title}" }
                            }
                        }
                    )
                })
            }
        }
    ))
}

#[inline_props]
fn arrow_down<'a>(cx: Scope, class: Option<&'a str>) -> Element {
    cx.render(rsx!(
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
    ))
}

fn is_dropdown_opened(has_value: bool, autofocus: bool, dropdown_opened: bool) -> bool {
    (!has_value && autofocus) || dropdown_opened
}

#[inline_props]
pub fn error_message<'a>(cx: Scope, message: &'a UseState<Option<String>>) -> Element {
    message.as_ref().and_then(|error| {
        cx.render(rsx!(
            p {
                class: "mt-2 text-error dark:text-error",
                span { class: "font-medium", "{error}" }
            }
        ))
    })
}

fn validate_value<T>(value: &str, error_message: UseState<Option<String>>, required: bool)
where
    T: FromStr,
    <T as FromStr>::Err: Display,
{
    if !required && value.is_empty() {
        error_message.set(None);
    } else {
        error_message.set(T::from_str(value).err().map(|error| error.to_string()));
    }
}

#[cfg(test)]
mod tests {
    mod compute_dropdown_style {
        use super::super::*;
        use rstest::*;

        #[rstest]
        fn test_has_no_value_and_autofocus() {
            assert!(is_dropdown_opened(false, true, false));
        }

        #[rstest]
        fn test_has_no_value_and_not_autofocus() {
            assert!(!is_dropdown_opened(false, false, false));
        }

        #[rstest]
        fn test_has_no_value_and_not_autofocus_and_opened() {
            assert!(is_dropdown_opened(false, false, true));
        }

        #[rstest]
        fn test_has_value_and_opened() {
            assert!(is_dropdown_opened(true, false, true));
        }

        #[rstest]
        fn test_has_value_and_not_opened() {
            assert!(!is_dropdown_opened(true, false, false));
        }
    }
}
