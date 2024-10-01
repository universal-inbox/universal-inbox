#![allow(non_snake_case)]

use dioxus::prelude::*;

use web_sys::KeyboardEvent;

#[derive(Clone)]
pub struct KeyboardManager<'a> {
    pub active_keyboard_handler: Option<&'a dyn KeyboardHandler>,
}

impl KeyboardManager<'_> {
    pub fn new() -> Self {
        Self {
            active_keyboard_handler: None,
        }
    }
}

pub trait KeyboardHandler {
    fn handle_keydown(&self, event: &KeyboardEvent) -> bool;
}

impl KeyboardHandler for KeyboardManager<'_> {
    fn handle_keydown(&self, event: &KeyboardEvent) -> bool {
        if let Some(handler) = &self.active_keyboard_handler {
            handler.handle_keydown(event)
        } else {
            false
        }
    }
}

pub static KEYBOARD_MANAGER: GlobalSignal<KeyboardManager> = Signal::global(KeyboardManager::new);
