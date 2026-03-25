use wasm_bindgen::prelude::*;
use web_sys::Element;

#[wasm_bindgen(module = "/public/js/index.js")]
extern "C" {
    pub fn init_flyonui_tabs_element(element: &Element);
    pub fn forget_flyonui_tabs_element(element: &Element);

    pub fn init_flyonui_modal(element: &Element);
    pub fn forget_flyonui_modal(element: &Element);
    pub fn has_flyonui_modal_opened() -> bool;
    pub fn open_flyonui_modal(target: &str);
    pub fn close_flyonui_modal(target: &str);

    pub fn init_flyonui_tooltip_element(element: &Element);
    pub fn forget_flyonui_tooltip_element(element: &Element);
}
