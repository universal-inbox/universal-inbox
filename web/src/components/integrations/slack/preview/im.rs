#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::notification::integrations::slack::SlackImDetails;

#[component]
pub fn SlackImPreview<'a>(cx: Scope, _slack_im: &'a SlackImDetails) -> Element {
    None
}
