#![allow(non_snake_case)]

use std::{fmt::Display, marker::PhantomData, str::FromStr};

use dioxus::prelude::*;
use log::error;

use crate::utils::focus_and_select_input_element;

#[derive(Props, Clone, PartialEq)]
pub struct InputProps<T: Clone + PartialEq + 'static> {
    name: ReadSignal<String>,
    #[props(!optional)]
    label: ReadSignal<Option<String>>,
    required: Option<bool>,
    value: Signal<String>,
    #[props(default)]
    autofocus: Option<bool>,
    #[props(default)]
    force_validation: ReadSignal<bool>,
    #[props(default)]
    disabled: Option<bool>,
    #[props(default)]
    r#type: Option<String>,
    #[props(default)]
    icon: Option<Element>,
    /// Tailwind iconify class (e.g. `"icon-[lucide--mail]"`) for a leading
    /// field icon. When set, the input renders the design-system `.field`
    /// markup instead of the simpler fallback markup.
    #[props(default)]
    field_icon_class: Option<String>,
    /// Optional aside element rendered to the right of the field label
    /// (typically a small "Forgot?" link).
    #[props(default)]
    aside: Option<Element>,
    /// Small muted helper text rendered under the field when there's no error.
    #[props(default)]
    help: Option<String>,
    /// Placeholder for the input. Defaults to a single space so the
    /// floating-label CSS rule keeps matching.
    #[props(default)]
    placeholder: Option<String>,
    #[props(default)]
    on_update: EventHandler<String>,
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
    let has_field_icon = props.field_icon_class.is_some();

    let error_message = use_signal(|| None);
    let mut show_password = use_signal(|| false);

    let input_type = props.r#type.clone().unwrap_or("text".to_string());
    let is_password = input_type == "password";
    let real_input_type = use_memo(move || {
        if is_password && show_password() {
            "text".to_string()
        } else {
            input_type.clone()
        }
    });

    let mut validate = use_signal(|| false);
    let _ = use_memo(move || {
        let force = (props.force_validation)();
        if force || validate() {
            validate_value::<T>(&(*props.value)(), error_message, required, force);
        }
    });

    let _resource = use_resource(move || async move {
        if props.autofocus.unwrap_or_default() {
            let name = (props.name)();
            if let Err(error) = focus_and_select_input_element(&name).await {
                error!("Error focusing element task-project-input: {error:?}");
            }
        }
    });

    if has_field_icon {
        // ── Tailwind + FlyonUI design-system markup ───────────────────────
        //
        // 40px filled input with `bg-ui-surface-alt` default, hover lifts the
        // border, focus swaps to `bg-ui-surface` + primary border + focus halo.
        // Composing the look from utilities keeps tokens reactive in dark mode.
        let icon_class = props
            .field_icon_class
            .clone()
            .unwrap_or_else(|| "icon-[lucide--circle]".to_string());
        let placeholder = props.placeholder.clone().unwrap_or_default();
        let label_text = (props.label)();
        let aside = props.aside.clone();
        let help = props.help.clone();
        let is_error = error_message().is_some();
        let input_state_classes = if is_error {
            // Error state — colored border + tinted bg, plus a softer error halo on focus.
            "bg-ui-error-subtle border-ui-error-hover focus:border-ui-error-hover focus:shadow-[0_0_0_2px_rgba(220,107,126,0.15)]"
        } else {
            // Default — filled chip on rest, primary border + focus halo on focus.
            "bg-ui-surface-alt border-ui-border focus:border-ui-primary focus:bg-ui-surface focus:shadow-[var(--ui-focus-ring)]"
        };

        rsx! {
            div { class: "flex flex-col gap-1.5 mb-4",

                if let Some(label) = label_text {
                    label {
                        class: "flex items-baseline justify-between text-[11.5px] font-semibold uppercase tracking-[0.06em] text-ui-base-muted",
                        "for": "{props.name}",

                        span {
                            "{label}"
                            if required {
                                span { class: "text-ui-error-hover ml-0.5", "*" }
                            }
                        }
                        if let Some(aside) = aside {
                            { aside }
                        }
                    }
                }

                div { class: "relative",
                    span {
                        class: "pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 inline-flex items-center text-ui-base-muted",
                        span { class: "{icon_class} size-4" }
                    }
                    input {
                        class: "block w-full h-10 pl-[38px] pr-3 outline-none font-ui text-[13.5px] text-ui-base-content border rounded-ui-sm transition-colors focus:outline-none focus-visible:outline-none placeholder:text-ui-base-muted placeholder:opacity-70 {input_state_classes}",
                        "type": "{real_input_type}",
                        name: "{props.name}",
                        id: "{props.name}",
                        placeholder,
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
                            props.on_update.call(props.value.read().clone());
                        },
                        autofocus: props.autofocus.unwrap_or_default(),
                        disabled: props.disabled.unwrap_or_default(),
                    }
                    if is_password {
                        button {
                            class: "absolute right-2 top-1/2 -translate-y-1/2 inline-flex items-center justify-center p-1.5 rounded-ui-xs text-ui-base-muted hover:text-ui-base-content hover:bg-ui-base-200 cursor-pointer",
                            "type": "button",
                            tabindex: "-1",
                            "aria-label": if show_password() { "Hide password" } else { "Show password" },
                            onclick: move |_| {
                                let current = show_password();
                                *show_password.write() = !current;
                            },
                            span {
                                class: if show_password() { "icon-[lucide--eye-off] size-4" } else { "icon-[lucide--eye] size-4" }
                            }
                        }
                    }
                }

                if let Some(error) = error_message() {
                    div {
                        id: "{props.name}-error",
                        class: "flex items-center gap-1.5 text-xs font-medium text-ui-error-hover",
                        role: "alert",
                        span { class: "icon-[lucide--alert-circle] size-3.5" }
                        span { "{error}" }
                    }
                } else if let Some(help) = help {
                    div { class: "text-xs text-ui-base-muted leading-snug", "{help}" }
                }
            }
        }
    } else {
        // ── Fallback markup — used by non-auth callers (datepicker,
        //    auth_methods_card, etc.) that don't pass a `field_icon_class`.
        let required_label_style = if required {
            "after:content-['*'] after:ml-0.5 after:text-error"
        } else {
            Default::default()
        };
        let has_icon = props.icon.is_some();
        let input_style = use_memo(move || error_message().and(Some("is-invalid")).unwrap_or(""));
        let placeholder = props.placeholder.clone().unwrap_or_else(|| " ".to_string());

        rsx! {
            div {
                style: "display:flex; flex-direction:column; gap:4px;",

                if let Some(label) = (props.label)() {
                    label {
                        "for": "{props.name}",
                        style: "font-size:var(--ui-text-sm); font-weight:500; color:var(--ui-base-muted);",
                        class: "{required_label_style}",
                        "{label}"
                    }
                }

                div {
                    style: "position:relative; display:flex; align-items:center;",

                    if let Some(icon) = &props.icon {
                        div {
                            style: "position:absolute; left:10px; color:var(--ui-base-muted); display:flex; align-items:center;",
                            { icon }
                        }
                    }

                    input {
                        class: "w-full px-3 py-2 text-[var(--ui-text-base)] font-ui bg-ui-surface text-ui-base-content border border-ui-border rounded-ui-sm outline-none transition-[border-color,box-shadow] duration-150 ease-[var(--ui-ease)] focus:border-ui-primary focus:shadow-[var(--ui-focus-ring)] focus:outline-none {input_style}",
                        style: if has_icon { "padding-left:34px;" } else { "" },
                        "type": "{real_input_type}",
                        name: "{props.name}",
                        id: "{props.name}",
                        placeholder,
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
                            props.on_update.call(props.value.read().clone());
                        },
                        autofocus: props.autofocus.unwrap_or_default(),
                        disabled: props.disabled.unwrap_or_default(),
                    }
                }
                ErrorMessage { message: error_message }
            }
        }
    }
}

#[component]
pub fn ErrorMessage(message: ReadSignal<Option<String>>) -> Element {
    if let Some(error) = message() {
        rsx! { span { class: "helper-text ps-3", "{error} "} }
    } else {
        rsx! {}
    }
}

fn validate_value<T>(
    value: &str,
    mut error_message: Signal<Option<String>>,
    required: bool,
    force: bool,
) where
    T: FromStr,
    <T as FromStr>::Err: Display,
{
    if value.is_empty() {
        // Only flag a missing required value once the user actually tries to
        // submit the form — focus-out alone shouldn't shout at an empty field.
        if required && force {
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
