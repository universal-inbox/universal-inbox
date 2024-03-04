#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::notification::integrations::slack::SlackFileDetails;

#[component]
pub fn SlackFilePreview<'a>(cx: Scope, _slack_file: &'a SlackFileDetails) -> Element {
    None
}
