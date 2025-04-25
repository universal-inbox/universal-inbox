#![allow(non_snake_case)]

use std::{fmt::Display, marker::PhantomData, str::FromStr};

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsCalendarEvent, Icon};
use log::error;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use web_sys::HtmlInputElement;

use crate::{
    components::floating_label_inputs::FloatingLabelInputText, utils::wait_for_element_by_id,
};

#[derive(Serialize, Deserialize)]
struct DatepickerOptions {
    pub autohide: bool,
    #[serde(rename = "todayBtn")]
    pub today_button: bool,
    #[serde(rename = "todayBtnMode")]
    pub today_button_mode: u8,
    #[serde(rename = "todayHighlight")]
    pub today_highlight: bool,
    pub format: String,
    #[serde(rename = "buttonClass")]
    pub button_class: String,
}

#[wasm_bindgen(module = "/public/js/index.js")]
extern "C" {
    type Datepicker;

    #[wasm_bindgen(constructor)]
    fn new(datepicker: web_sys::HtmlInputElement, options: JsValue) -> Datepicker;

    fn flatpickr(flatpickr: web_sys::HtmlInputElement);
}

#[derive(Props, Clone, PartialEq)]
pub struct DatePickerProps<T: Clone + PartialEq + 'static> {
    name: String,
    label: Option<String>,
    required: bool,
    value: Signal<String>,
    #[props(default)]
    autofocus: bool,
    #[props(default)]
    force_validation: bool,
    #[props(default)]
    phantom: PhantomData<T>,
}

#[component]
pub fn DatePicker<T>(props: DatePickerProps<T>) -> Element
where
    T: FromStr + Clone + PartialEq,
    <T as FromStr>::Err: Display,
{
    let name = props.name.clone();
    let _ = use_resource(move || {
        to_owned![name];
        async move {
            // Initialize datepicker element
            let Ok(element) = wait_for_element_by_id(&name, 300).await else {
                error!("Element `{}` not found", &name);
                return;
            };
            let element = element.dyn_into::<HtmlInputElement>().unwrap();
            flatpickr(element);
        }
    });

    let icon = rsx! { Icon { icon: BsCalendarEvent } };

    rsx! {
        FloatingLabelInputText::<T> {
            name: props.name,
            label: props.label,
            icon: icon,
            required: props.required,
            value: props.value,
            autofocus: props.autofocus,
            force_validation: props.force_validation,
        }
    }
}
