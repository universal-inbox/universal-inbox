#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsBookmarkFill, Icon};
use slack_morphism::prelude::{SlackEventCallbackBody, SlackPushEventCallback};

#[component]
pub fn SlackNotificationIcon<'a>(
    cx: Scope,
    slack_push_event_callback: &'a SlackPushEventCallback,
    class: Option<&'a str>,
) -> Element {
    let class = class.unwrap_or_default();
    match slack_push_event_callback.event {
        SlackEventCallbackBody::StarAdded(_) | SlackEventCallbackBody::StarRemoved(_) => render! {
            Icon { class: "{class}", icon: BsBookmarkFill }
        },
        _ => None,
    }
}
