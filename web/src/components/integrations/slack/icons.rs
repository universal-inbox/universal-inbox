#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{Icon, icons::bs_icons::BsBookmarkFill};

#[component]
pub fn SlackNotificationIcon(class: Option<String>) -> Element {
    let class = class.unwrap_or_default();
    rsx! { Icon { class: "{class}", icon: BsBookmarkFill } }
}
