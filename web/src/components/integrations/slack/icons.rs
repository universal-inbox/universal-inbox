#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsBookmarkFill, Icon};
use slack_morphism::prelude::{SlackEventCallbackBody, SlackPushEventCallback};

#[component]
pub fn SlackNotificationIcon(
    slack_push_event_callback: ReadOnlySignal<SlackPushEventCallback>,
    class: Option<String>,
) -> Element {
    let class = class.unwrap_or_default();
    match slack_push_event_callback().event {
        SlackEventCallbackBody::StarAdded(_) | SlackEventCallbackBody::StarRemoved(_) => rsx! {
            Icon { class: "{class}", icon: BsBookmarkFill }
        },
        _ => None,
    }
}
