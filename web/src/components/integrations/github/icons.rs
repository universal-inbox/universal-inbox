#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::third_party::integrations::github::{
    GithubDiscussion, GithubDiscussionStateReason, GithubPullRequest, GithubPullRequestState,
};

use crate::theme::{
    CANCELED_TEXT_COLOR_CLASS, COMPLETED_TEXT_COLOR_CLASS, DRAFT_TEXT_COLOR_CLASS,
    STARTED_TEXT_COLOR_CLASS,
};

#[component]
pub fn Github(class: Option<String>) -> Element {
    let class = class.unwrap_or_default();
    // `dark:invert` flips the monochrome Octocat to white in dark mode so
    // it stays visible on dark surfaces.
    rsx! { span { class: "icon-[logos--github-icon] dark:invert {class}" } }
}

#[component]
pub fn GithubPullRequestIcon(
    github_pull_request: Option<GithubPullRequest>,
    class: Option<String>,
    should_style_icon: Option<bool>,
) -> Element {
    let (closed_icon_style, merged_icon_style, draft_icon_style, opened_icon_style) =
        if should_style_icon.unwrap_or(true) {
            (
                CANCELED_TEXT_COLOR_CLASS,
                COMPLETED_TEXT_COLOR_CLASS,
                DRAFT_TEXT_COLOR_CLASS,
                STARTED_TEXT_COLOR_CLASS,
            )
        } else {
            ("", "", "", "")
        };
    let class = class.unwrap_or_default();
    let Some(github_pull_request) = github_pull_request else {
        return rsx! { span { class: "icon-[lucide--git-pull-request] {class}" } };
    };

    match github_pull_request.state {
        GithubPullRequestState::Closed => {
            rsx! { span { class: "icon-[lucide--git-pull-request-closed] {class} {closed_icon_style}" } }
        }
        GithubPullRequestState::Merged => {
            rsx! { span { class: "icon-[lucide--git-pull-request] {class} {merged_icon_style}" } }
        }
        GithubPullRequestState::Open => {
            if github_pull_request.is_draft {
                rsx! { span { class: "icon-[lucide--git-pull-request-draft] {class} {draft_icon_style}" } }
            } else {
                rsx! { span { class: "icon-[lucide--git-pull-request] {class} {opened_icon_style}" } }
            }
        }
    }
}

#[component]
pub fn GithubDiscussionOpened(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: class.unwrap_or_default(),
            role: "img",
            "viewBox": "0 0 24 24",
            fill: "currentColor",
            stroke: "currentColor",
            title { "Github opened discussion" }
            path {
                d: "M1.75 1h12.5c.966 0 1.75.784 1.75 1.75v9.5A1.75 1.75 0 0 1 14.25 14H8.061l-2.574 2.573A1.458 1.458 0 0 1 3 15.543V14H1.75A1.75 1.75 0 0 1 0 12.25v-9.5C0 1.784.784 1 1.75 1ZM1.5 2.75v9.5c0 .138.112.25.25.25h2a.75.75 0 0 1 .75.75v2.19l2.72-2.72a.749.749 0 0 1 .53-.22h6.5a.25.25 0 0 0 .25-.25v-9.5a.25.25 0 0 0-.25-.25H1.75a.25.25 0 0 0-.25.25Z"
            }
            path {
                d: "M22.5 8.75a.25.25 0 0 0-.25-.25h-3.5a.75.75 0 0 1 0-1.5h3.5c.966 0 1.75.784 1.75 1.75v9.5A1.75 1.75 0 0 1 22.25 20H21v1.543a1.457 1.457 0 0 1-2.487 1.03L15.939 20H10.75A1.75 1.75 0 0 1 9 18.25v-1.465a.75.75 0 0 1 1.5 0v1.465c0 .138.112.25.25.25h5.5a.75.75 0 0 1 .53.22l2.72 2.72v-2.19a.75.75 0 0 1 .75-.75h2a.25.25 0 0 0 .25-.25v-9.5Z"
            }
        }
    }
}

#[component]
pub fn GithubDiscussionClosed(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: class.unwrap_or_default(),
            role: "img",
            "viewBox": "0 0 24 24",
            fill: "currentColor",
            stroke: "currentColor",
            title { "Github closed discussion" }
            path  {
                d: "M0 2.75C0 1.783.784 1 1.75 1h12.5c.967 0 1.75.783 1.75 1.75v9.5A1.75 1.75 0 0 1 14.25 14H8.061l-2.574 2.573A1.457 1.457 0 0 1 3 15.543V14H1.75A1.75 1.75 0 0 1 0 12.25Zm1.75-.25a.25.25 0 0 0-.25.25v9.5c0 .138.112.25.25.25h2a.75.75 0 0 1 .75.75v2.189l2.72-2.719a.747.747 0 0 1 .53-.22h6.5a.25.25 0 0 0 .25-.25v-9.5a.25.25 0 0 0-.25-.25Zm20.5 6h-3.5a.75.75 0 0 1 0-1.5h3.5c.966 0 1.75.784 1.75 1.75v9.5A1.75 1.75 0 0 1 22.25 20H21v1.543a1.457 1.457 0 0 1-2.487 1.03L15.939 20H10.75A1.75 1.75 0 0 1 9 18.25v-1.465a.75.75 0 0 1 1.5 0v1.465c0 .138.112.25.25.25h5.5c.199 0 .39.079.53.22l2.72 2.719V19.25a.75.75 0 0 1 .75-.75h2a.25.25 0 0 0 .25-.25v-9.5a.25.25 0 0 0-.25-.25Zm-9.72-3.22-5 5a.747.747 0 0 1-1.06 0l-2.5-2.5a.749.749 0 1 1 1.06-1.06L7 8.689l4.47-4.469a.749.749 0 1 1 1.06 1.06Z"
            }
        }
    }
}

#[component]
pub fn GithubDiscussionDuplicate(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: class.unwrap_or_default(),
            role: "img",
            "viewBox": "0 0 24 24",
            fill: "currentColor",
            stroke: "currentColor",
            title { "Github duplicate discussion" }
            path {
                d: "M0 2.75C0 1.783.784 1 1.75 1h12.5c.967 0 1.75.783 1.75 1.75v9.5A1.75 1.75 0 0 1 14.25 14H8.061l-2.574 2.573A1.457 1.457 0 0 1 3 15.543V14H1.75A1.75 1.75 0 0 1 0 12.25Zm1.75-.25a.25.25 0 0 0-.25.25v9.5c0 .138.112.25.25.25h2a.75.75 0 0 1 .75.75v2.189l2.72-2.719a.747.747 0 0 1 .53-.22h6.5a.25.25 0 0 0 .25-.25v-9.5a.25.25 0 0 0-.25-.25Zm20.5 6h-3.5a.75.75 0 0 1 0-1.5h3.5c.966 0 1.75.784 1.75 1.75v9.5A1.75 1.75 0 0 1 22.25 20H21v1.543a1.457 1.457 0 0 1-2.487 1.03L15.939 20H10.75A1.75 1.75 0 0 1 9 18.25v-1.465a.75.75 0 0 1 1.5 0v1.465c0 .138.112.25.25.25h5.5c.199 0 .39.079.53.22l2.72 2.719V19.25a.75.75 0 0 1 .75-.75h2a.25.25 0 0 0 .25-.25v-9.5a.25.25 0 0 0-.25-.25ZM11.28 5.53l-5 5a.749.749 0 1 1-1.06-1.06l5-5a.749.749 0 1 1 1.06 1.06Z"
            }
        }
    }
}

#[component]
pub fn GithubDiscussionOutdated(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: class.unwrap_or_default(),
            role: "img",
            "viewBox": "0 0 24 24",
            fill: "currentColor",
            stroke: "currentColor",
            title { "Github outdated discussion" }
            path {
                d: "M0 2.75C0 1.783.784 1 1.75 1h12.5c.967 0 1.75.783 1.75 1.75v9.5A1.75 1.75 0 0 1 14.25 14H8.061l-2.574 2.573A1.457 1.457 0 0 1 3 15.543V14H1.75A1.75 1.75 0 0 1 0 12.25Zm1.75-.25a.25.25 0 0 0-.25.25v9.5c0 .138.112.25.25.25h2a.75.75 0 0 1 .75.75v2.189l2.72-2.719a.747.747 0 0 1 .53-.22h6.5a.25.25 0 0 0 .25-.25v-9.5a.25.25 0 0 0-.25-.25Zm20.5 6h-3.5a.75.75 0 0 1 0-1.5h3.5c.966 0 1.75.784 1.75 1.75v9.5A1.75 1.75 0 0 1 22.25 20H21v1.543a1.457 1.457 0 0 1-2.487 1.03L15.939 20H10.75A1.75 1.75 0 0 1 9 18.25v-1.465a.75.75 0 0 1 1.5 0v1.465c0 .138.112.25.25.25h5.5c.199 0 .39.079.53.22l2.72 2.719V19.25a.75.75 0 0 1 .75-.75h2a.25.25 0 0 0 .25-.25v-9.5a.25.25 0 0 0-.25-.25ZM8.5 4.75v3.14l1.15.488a.608.608 0 0 1 .037.017l1.393.681a.75.75 0 0 1-.66 1.348l-1.374-.673-1.589-.674A.751.751 0 0 1 7 8.386V4.75a.75.75 0 0 1 1.5 0Z"
            }
        }
    }
}

#[component]
pub fn GithubDiscussionIcon(
    github_discussion: Option<GithubDiscussion>,
    class: Option<String>,
    should_style_icon: Option<bool>,
) -> Element {
    let (closed_icon_style, opened_icon_style, duplicate_icon_style, outdated_icon_style) =
        if should_style_icon.unwrap_or(true) {
            (
                COMPLETED_TEXT_COLOR_CLASS,
                STARTED_TEXT_COLOR_CLASS,
                CANCELED_TEXT_COLOR_CLASS,
                CANCELED_TEXT_COLOR_CLASS,
            )
        } else {
            ("", "", "", "")
        };
    let class = class.unwrap_or_default();

    if let Some(github_discussion) = github_discussion {
        return match github_discussion.state_reason {
            Some(GithubDiscussionStateReason::Duplicate) => rsx! {
                GithubDiscussionDuplicate { class: "{class} {duplicate_icon_style}" }
            },
            Some(GithubDiscussionStateReason::Outdated) => rsx! {
                GithubDiscussionOutdated { class: "{class} {outdated_icon_style}" }
            },
            Some(GithubDiscussionStateReason::Reopened) => rsx! {
                GithubDiscussionOpened { class: "{class} {opened_icon_style}" }
            },
            Some(GithubDiscussionStateReason::Resolved) => rsx! {
                GithubDiscussionClosed { class: "{class} {closed_icon_style}" }
            },
            _ => rsx! {
                GithubDiscussionOpened { class: "{class} {opened_icon_style}" }
            },
        };
    }

    rsx! { GithubDiscussionOpened { class: "{class} {opened_icon_style}" } }
}
