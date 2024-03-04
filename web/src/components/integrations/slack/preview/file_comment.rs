#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::notification::integrations::slack::SlackFileCommentDetails;

#[component]
pub fn SlackFileCommentPreview<'a>(
    cx: Scope,
    _slack_file_comment: &'a SlackFileCommentDetails,
) -> Element {
    None
}
