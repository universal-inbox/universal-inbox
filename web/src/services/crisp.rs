use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/public/js/index.js")]
extern "C" {
    pub fn init_crisp(
        website_id: &str,
        user_email: Option<&str>,
        user_email_signature: Option<&str>,
        user_nickname: Option<&str>,
        user_avatar: Option<&str>,
        user_id: Option<&str>,
    );
    pub fn unload_crisp();
    pub fn is_crisp_chat_opened() -> bool;
}
