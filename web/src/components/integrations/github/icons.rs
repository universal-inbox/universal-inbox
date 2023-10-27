#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::notification::integrations::github::{
    GithubPullRequest, GithubPullRequestState,
};

#[inline_props]
pub fn Github<'a>(cx: Scope, class: Option<&'a str>) -> Element {
    render! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: "{class.unwrap_or_default()}",
            role: "img",
            "viewBox": "0 0 24 24",
            fill: "currentColor",
            stroke: "currentColor",
            title { "Github" }
            path {
                d: "M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12"
            }
        }
    }
}

#[inline_props]
pub fn GithubPullRequestOpened<'a>(cx: Scope, class: Option<&'a str>) -> Element {
    render! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: "{class.unwrap_or_default()}",
            role: "img",
            "viewBox": "0 0 24 24",
            fill: "currentColor",
            stroke: "currentColor",
            title { "Github opened pull request" }
            path {
                d: "M16 19.25a3.25 3.25 0 1 1 6.5 0 3.25 3.25 0 0 1-6.5 0Zm-14.5 0a3.25 3.25 0 1 1 6.5 0 3.25 3.25 0 0 1-6.5 0Zm0-14.5a3.25 3.25 0 1 1 6.5 0 3.25 3.25 0 0 1-6.5 0ZM4.75 3a1.75 1.75 0 1 0 .001 3.501A1.75 1.75 0 0 0 4.75 3Zm0 14.5a1.75 1.75 0 1 0 .001 3.501A1.75 1.75 0 0 0 4.75 17.5Zm14.5 0a1.75 1.75 0 1 0 .001 3.501 1.75 1.75 0 0 0-.001-3.501Z"
            }
            path {
                d: "M13.405 1.72a.75.75 0 0 1 0 1.06L12.185 4h4.065A3.75 3.75 0 0 1 20 7.75v8.75a.75.75 0 0 1-1.5 0V7.75a2.25 2.25 0 0 0-2.25-2.25h-4.064l1.22 1.22a.75.75 0 0 1-1.061 1.06l-2.5-2.5a.75.75 0 0 1 0-1.06l2.5-2.5a.75.75 0 0 1 1.06 0ZM4.75 7.25A.75.75 0 0 1 5.5 8v8A.75.75 0 0 1 4 16V8a.75.75 0 0 1 .75-.75Z"
            }
        }
    }
}

#[inline_props]
pub fn GithubPullRequestDraft<'a>(cx: Scope, class: Option<&'a str>) -> Element {
    render! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: "{class.unwrap_or_default()}",
            role: "img",
            "viewBox": "0 0 24 24",
            fill: "currentColor",
            stroke: "currentColor",
            title { "Github draft pull request" }
            path {
                d: "M4.75 1.5a3.25 3.25 0 0 1 .745 6.414A.827.827 0 0 1 5.5 8v8a.827.827 0 0 1-.005.086A3.25 3.25 0 0 1 4.75 22.5a3.25 3.25 0 0 1-.745-6.414A.827.827 0 0 1 4 16V8c0-.029.002-.057.005-.086A3.25 3.25 0 0 1 4.75 1.5ZM16 19.25a3.25 3.25 0 1 1 6.5 0 3.25 3.25 0 0 1-6.5 0ZM3 4.75a1.75 1.75 0 1 0 3.501-.001A1.75 1.75 0 0 0 3 4.75Zm0 14.5a1.75 1.75 0 1 0 3.501-.001A1.75 1.75 0 0 0 3 19.25Zm16.25-1.75a1.75 1.75 0 1 0 .001 3.501 1.75 1.75 0 0 0-.001-3.501Zm0-11.5a1.75 1.75 0 1 0 0-3.5 1.75 1.75 0 0 0 0 3.5ZM21 11.25a1.75 1.75 0 1 1-3.5 0 1.75 1.75 0 0 1 3.5 0Z"
            }
        }
    }
}

#[inline_props]
pub fn GithubPullRequestClosed<'a>(cx: Scope, class: Option<&'a str>) -> Element {
    render! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: "{class.unwrap_or_default()}",
            role: "img",
            "viewBox": "0 0 24 24",
            fill: "currentColor",
            stroke: "currentColor",
            title { "Github closed pull request" }
            path {
                d: "M22.266 2.711a.75.75 0 1 0-1.061-1.06l-1.983 1.983-1.984-1.983a.75.75 0 1 0-1.06 1.06l1.983 1.983-1.983 1.984a.75.75 0 0 0 1.06 1.06l1.984-1.983 1.983 1.983a.75.75 0 0 0 1.06-1.06l-1.983-1.984 1.984-1.983ZM4.75 1.5a3.25 3.25 0 0 1 .745 6.414A.827.827 0 0 1 5.5 8v8a.827.827 0 0 1-.005.086A3.25 3.25 0 0 1 4.75 22.5a3.25 3.25 0 0 1-.745-6.414A.827.827 0 0 1 4 16V8c0-.029.002-.057.005-.086A3.25 3.25 0 0 1 4.75 1.5ZM16 19.25a3.252 3.252 0 0 1 2.5-3.163V9.625a.75.75 0 0 1 1.5 0v6.462a3.252 3.252 0 0 1-.75 6.413A3.25 3.25 0 0 1 16 19.25ZM3 4.75a1.75 1.75 0 1 0 3.501-.001A1.75 1.75 0 0 0 3 4.75Zm0 14.5a1.75 1.75 0 1 0 3.501-.001A1.75 1.75 0 0 0 3 19.25Zm16.25-1.75a1.75 1.75 0 1 0 .001 3.501 1.75 1.75 0 0 0-.001-3.501Z"
            }
        }
    }
}

#[inline_props]
pub fn GithubPullRequestMerged<'a>(cx: Scope, class: Option<&'a str>) -> Element {
    render! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: "{class.unwrap_or_default()}",
            role: "img",
            "viewBox": "0 0 24 24",
            fill: "currentColor",
            stroke: "currentColor",
            title { "Github merged pull request" }
            path {
                d: "M16 19.25a3.25 3.25 0 1 1 6.5 0 3.25 3.25 0 0 1-6.5 0Zm-14.5 0a3.25 3.25 0 1 1 6.5 0 3.25 3.25 0 0 1-6.5 0Zm0-14.5a3.25 3.25 0 1 1 6.5 0 3.25 3.25 0 0 1-6.5 0ZM4.75 3a1.75 1.75 0 1 0 .001 3.501A1.75 1.75 0 0 0 4.75 3Zm0 14.5a1.75 1.75 0 1 0 .001 3.501A1.75 1.75 0 0 0 4.75 17.5Zm14.5 0a1.75 1.75 0 1 0 .001 3.501 1.75 1.75 0 0 0-.001-3.501Z"
            }
            path {
                d: "M13.405 1.72a.75.75 0 0 1 0 1.06L12.185 4h4.065A3.75 3.75 0 0 1 20 7.75v8.75a.75.75 0 0 1-1.5 0V7.75a2.25 2.25 0 0 0-2.25-2.25h-4.064l1.22 1.22a.75.75 0 0 1-1.061 1.06l-2.5-2.5a.75.75 0 0 1 0-1.06l2.5-2.5a.75.75 0 0 1 1.06 0ZM4.75 7.25A.75.75 0 0 1 5.5 8v8A.75.75 0 0 1 4 16V8a.75.75 0 0 1 .75-.75Z"
            }
        }
    }
}

#[inline_props]
pub fn GithubPullRequestIcon<'a>(
    cx: Scope,
    github_pull_request: Option<&'a GithubPullRequest>,
    class: Option<&'a str>,
    should_style_icon: Option<bool>,
) -> Element {
    let (closed_icon_style, merged_icon_style, draft_icon_style, opened_icon_style) =
        if should_style_icon.unwrap_or(true) {
            (
                "text-base",
                "text-indigo-500",
                "text-gray-400",
                "text-primary",
            )
        } else {
            ("", "", "", "")
        };
    let class = class.unwrap_or_default();
    let Some(github_pull_request) = github_pull_request else {
        return render! { GithubPullRequestOpened { class: "{class}" } };
    };

    match github_pull_request.state {
        GithubPullRequestState::Closed => {
            render! { GithubPullRequestClosed { class: "{class} {closed_icon_style}" }}
        }
        GithubPullRequestState::Merged => {
            render! { GithubPullRequestMerged { class: "{class} {merged_icon_style}" }}
        }
        GithubPullRequestState::Open => {
            if github_pull_request.is_draft {
                render! { GithubPullRequestDraft { class: "{class} {draft_icon_style}" }}
            } else {
                render! { GithubPullRequestOpened { class: "{class} {opened_icon_style}" }}
            }
        }
    }
}
