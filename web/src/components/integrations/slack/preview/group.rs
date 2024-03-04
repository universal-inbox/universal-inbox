#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::notification::integrations::slack::SlackGroupDetails;

#[component]
pub fn SlackGroupPreview<'a>(cx: Scope, _slack_group: &'a SlackGroupDetails) -> Element {
    None
}
