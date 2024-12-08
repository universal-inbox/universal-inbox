use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/dist/js/index.js")]
extern "C" {
    pub fn init_headway();
    pub fn show_headway();
}
