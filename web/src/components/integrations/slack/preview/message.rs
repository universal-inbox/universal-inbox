#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::notification::integrations::slack::SlackMessageDetails;

#[component]
pub fn SlackMessagePreview<'a>(cx: Scope, _slack_message: &'a SlackMessageDetails) -> Element {
    None
}
