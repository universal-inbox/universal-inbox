#![allow(non_snake_case)]

use dioxus::prelude::*;

use web_sys::KeyboardEvent;

use crate::services::{crisp::is_crisp_chat_opened, flyonui::has_flyonui_modal_opened};

#[derive(Clone)]
pub struct KeyboardManager {
    pub active_keyboard_handler: Option<&'static dyn KeyboardHandler>,
}

impl KeyboardManager {
    pub fn new() -> Self {
        Self {
            active_keyboard_handler: None,
        }
    }
}

pub trait KeyboardHandler {
    fn handle_keydown(&self, event: &KeyboardEvent) -> bool;
}

impl KeyboardHandler for KeyboardManager {
    fn handle_keydown(&self, event: &KeyboardEvent) -> bool {
        if has_flyonui_modal_opened() {
            return false;
        }
        if is_crisp_chat_opened() {
            return false;
        }
        if let Some(handler) = &self.active_keyboard_handler {
            handler.handle_keydown(event)
        } else {
            false
        }
    }
}

pub static KEYBOARD_MANAGER: GlobalSignal<KeyboardManager> = Signal::global(KeyboardManager::new);
