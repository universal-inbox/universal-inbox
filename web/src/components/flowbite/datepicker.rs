#![allow(non_snake_case)]
#![allow(clippy::empty_docs)] // Clippy raises on #[wasm_bindgen]?!?

use std::{fmt::Display, marker::PhantomData, str::FromStr};

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsCalendarEvent, Icon};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use web_sys::{CustomEvent, HtmlInputElement, InputEvent, InputEventInit};

use crate::{components::floating_label_inputs::FloatingLabelInputText, utils::get_element_by_id};

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

#[wasm_bindgen(module = "/js/index.js")]
extern "C" {
    type Datepicker;

    #[wasm_bindgen(constructor)]
    fn new(datepicker: web_sys::HtmlInputElement, options: JsValue) -> Datepicker;
}

#[derive(Props)]
pub struct DatePickerProps<T: 'static> {
    name: String,
    label: String,
    required: bool,
    value: UseState<String>,
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
pub fn DatePicker<T>(cx: Scope<DatePickerProps<T>>) -> Element
where
    T: FromStr,
    <T as FromStr>::Err: Display,
{
    let today_button = cx.props.today_button;
    let today_highlight = cx.props.today_highlight;
    let autohide = cx.props.autohide;
    let element_name = cx.props.name.clone();

    use_on_create(cx, || async move {
        // Initialize datepicker element
        let element = get_element_by_id(&element_name)
            .unwrap()
            .dyn_into::<HtmlInputElement>()
            .unwrap();
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
            let _ = element.dispatch_event(
                &InputEvent::new_with_event_init_dict(
                    "change",
                    InputEventInit::new()
                        .bubbles(true)
                        .cancelable(true)
                        .data(Some(element.value().as_str())),
                )
                .unwrap(),
            );
        });
        let _ = cloned_element
            .add_event_listener_with_callback("changeDate", closure.as_ref().unchecked_ref());
        closure.forget();
    });

    let icon = render! { Icon { icon: BsCalendarEvent } };

    render! {
        FloatingLabelInputText::<T> {
            name: cx.props.name.clone(),
            label: cx.props.label.clone(),
            icon: icon,
            required: cx.props.required,
            value: cx.props.value.clone(),
            autofocus: cx.props.autofocus,
            force_validation: cx.props.force_validation,
        }
    }
}
