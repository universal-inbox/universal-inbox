#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::notification::integrations::github::GithubDiscussion;

#[inline_props]
pub fn GithubDiscussionPreview<'a>(cx: Scope, _github_discussion: &'a GithubDiscussion) -> Element {
    render! {
        div {
        }
    }
}
