#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::notification::integrations::slack::SlackChannelDetails;

#[component]
pub fn SlackChannelPreview<'a>(cx: Scope, _slack_channel: &'a SlackChannelDetails) -> Element {
    None
}
