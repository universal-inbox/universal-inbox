#![allow(non_snake_case)]

use std::{fmt::Display, marker::PhantomData, str::FromStr};

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsCalendarEvent, Icon};
use log::error;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use web_sys::{CustomEvent, HtmlInputElement, InputEvent, InputEventInit};

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
}

#[derive(Props, Clone, PartialEq)]
pub struct DatePickerProps<T: Clone + PartialEq + 'static> {
    name: ReadOnlySignal<String>,
    label: Option<String>,
    required: bool,
    value: Signal<String>,
    #[props(default)]
    autofocus: bool,
    #[props(default)]
    force_validation: bool,
    #[props(default)]
    pub autohide: bool,
    #[props(default)]
    pub today_button: bool,
    #[props(default)]
    pub today_highlight: bool,
    #[props(default)]
    phantom: PhantomData<T>,
}

#[component]
pub fn DatePicker<T>(props: DatePickerProps<T>) -> Element
where
    T: FromStr + Clone + PartialEq,
    <T as FromStr>::Err: Display,
{
    let today_button = props.today_button;
    let today_highlight = props.today_highlight;
    let autohide = props.autohide;

    let _ = use_resource(move || async move {
        // Initialize datepicker element
        let Ok(element) = wait_for_element_by_id(&props.name.read(), 300).await else {
            error!("Element `{}` not found", &props.name.read());
            return;
        };
        let element = element.dyn_into::<HtmlInputElement>().unwrap();
        Datepicker::new(
            element.clone(),
            serde_wasm_bindgen::to_value(&DatepickerOptions {
                autohide,
                today_button,
                today_button_mode: if today_button { 1 } else { 0 },
                today_highlight,
                format: "yyyy-mm-dd".to_string(),
                // Override Flowbite default button class
                button_class: "btn btn-primary !text-black !border-0 !bg-primary hover:!bg-primary/90 !rounded".to_string(),
            })
            .unwrap(),
        );

        // Forward `changeDate` event to `change` event
        let cloned_element = element.clone();
        let closure = Closure::<dyn FnMut(_)>::new(move |_event: CustomEvent| {
            let input_event_init = InputEventInit::new();
            input_event_init.set_bubbles(true);
            input_event_init.set_cancelable(true);
            input_event_init.set_data(Some(element.value().as_str()));
            let _ = element.dispatch_event(
                &InputEvent::new_with_event_init_dict("change", &input_event_init).unwrap(),
            );
        });
        let _ = cloned_element
            .add_event_listener_with_callback("changeDate", closure.as_ref().unchecked_ref());
        closure.forget();
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
