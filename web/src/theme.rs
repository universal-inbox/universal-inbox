use log::debug;

use anyhow::{Context, Result};
use dioxus::prelude::*;
use gloo_utils::errors::JsError;

use crate::utils::get_local_storage;

// Notification state colors
pub const DRAFT_TEXT_COLOR_CLASS: &str = "text-gray-400";
pub const BACKLOG_TEXT_COLOR_CLASS: &str = "text-base";
pub const STARTED_TEXT_COLOR_CLASS: &str = "text-primary";
pub const COMPLETED_TEXT_COLOR_CLASS: &str = "text-indigo-500";
pub const CANCELED_TEXT_COLOR_CLASS: &str = "text-gray-400";

// Priorities colors
pub const PRIORITY_LOW_COLOR_CLASS: &str = "text-primary";
pub const PRIORITY_NORMAL_COLOR_CLASS: &str = "text-yellow-500";
pub const PRIORITY_HIGH_COLOR_CLASS: &str = "text-orange-500";
pub const PRIORITY_URGENT_COLOR_CLASS: &str = "text-red-500";

pub static IS_DARK_MODE: GlobalSignal<bool> = Signal::global(|| false);

pub fn toggle_dark_mode(toggle: bool) -> Result<bool> {
    let window = web_sys::window().context("Unable to get the window object")?;
    let document = window
        .document()
        .context("Unable to get the document object")?;
    let document_element = document
        .document_element()
        .context("Unable to get the document element")?;
    let local_storage = get_local_storage()?;

    let dark_mode = match local_storage.get_item("color-theme") {
        Ok(Some(value)) if value == *"dark" => true,
        Ok(Some(_)) => false,
        _ => matches!(
            window.match_media("(prefers-color-scheme: dark)"),
            Ok(Some(_))
        ),
    };

    let switch_to_dark_mode = (dark_mode && !toggle) || (!dark_mode && toggle);
    debug!("Switching dark mode {switch_to_dark_mode}");
    if switch_to_dark_mode {
        document_element
            .set_attribute("data-theme", "uidark")
            .map_err(|err| JsError::try_from(err).unwrap())?;
        document_element
            .class_list()
            .add_1("dark")
            .map_err(|err| JsError::try_from(err).unwrap())?;
        local_storage
            .set_item("color-theme", "dark")
            .map_err(|err| JsError::try_from(err).unwrap())?;
    } else {
        document_element
            .set_attribute("data-theme", "uilight")
            .map_err(|err| JsError::try_from(err).unwrap())?;
        document_element
            .class_list()
            .remove_1("dark")
            .map_err(|err| JsError::try_from(err).unwrap())?;
        local_storage
            .set_item("color-theme", "light")
            .map_err(|err| JsError::try_from(err).unwrap())?;
    }

    Ok(switch_to_dark_mode)
}
