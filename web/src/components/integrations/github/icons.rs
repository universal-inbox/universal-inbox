#![allow(non_snake_case)]

use dioxus::prelude::*;

use dioxus_free_icons::{
    icons::{
        bs_icons::{BsCheckCircle, BsRecordCircle},
        go_icons::{GoGitCommit, GoGitPullRequest, GoGitPullRequestClosed, GoGitPullRequestDraft},
    },
    Icon,
};

use universal_inbox::notification::{
    integrations::github::{
        GithubDiscussion, GithubDiscussionStateReason, GithubNotification, GithubPullRequest,
        GithubPullRequestState,
    },
    NotificationDetails, NotificationWithTask,
};

use crate::theme::{
    CANCELED_TEXT_COLOR_CLASS, COMPLETED_TEXT_COLOR_CLASS, DRAFT_TEXT_COLOR_CLASS,
    STARTED_TEXT_COLOR_CLASS,
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
pub fn GithubNotificationIcon<'a>(
    cx: Scope,
    notif: &'a NotificationWithTask,
    github_notification: &'a GithubNotification,
    class: Option<&'a str>,
) -> Element {
    let class = class.unwrap_or_default();
    match &notif.details {
        Some(NotificationDetails::GithubPullRequest(pr)) => render! {
            GithubPullRequestIcon { class: "{class}", github_pull_request: pr }
        },
        Some(NotificationDetails::GithubDiscussion(discussion)) => render! {
            GithubDiscussionIcon { class: "{class}", github_discussion: discussion }
        },
        None => match github_notification.subject.r#type.as_str() {
            "PullRequest" => render! { GithubPullRequestIcon { class: "{class}" } },
            "Issue" => render! { Icon { class: "{class}", icon: BsRecordCircle } },
            "Discussion" => render! { GithubDiscussionIcon { class: "{class}" } },
            "CheckSuite" => render! { Icon { class: "{class}", icon: BsCheckCircle } },
            "Commit" => render! { Icon { class: "{class}", icon: GoGitCommit } },
            _ => None,
        },
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
        return render! { Icon { class: "{class}", icon: GoGitPullRequest } };
    };

    match github_pull_request.state {
        GithubPullRequestState::Closed => {
            render! { Icon { class: "{class} {closed_icon_style}", icon: GoGitPullRequestClosed }}
        }
        GithubPullRequestState::Merged => {
            render! { Icon { class: "{class} {merged_icon_style}", icon: GoGitPullRequest }}
        }
        GithubPullRequestState::Open => {
            if github_pull_request.is_draft {
                render! { Icon { class: "{class} {draft_icon_style}", icon: GoGitPullRequestDraft }}
            } else {
                render! { Icon { class: "{class} {opened_icon_style}", icon: GoGitPullRequest }}
            }
        }
    }
}

#[inline_props]
pub fn GithubDiscussionOpened<'a>(cx: Scope, class: Option<&'a str>) -> Element {
    render! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: "{class.unwrap_or_default()}",
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

#[inline_props]
pub fn GithubDiscussionClosed<'a>(cx: Scope, class: Option<&'a str>) -> Element {
    render! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: "{class.unwrap_or_default()}",
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

#[inline_props]
pub fn GithubDiscussionDuplicate<'a>(cx: Scope, class: Option<&'a str>) -> Element {
    render! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: "{class.unwrap_or_default()}",
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

#[inline_props]
pub fn GithubDiscussionOutdated<'a>(cx: Scope, class: Option<&'a str>) -> Element {
    render! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: "{class.unwrap_or_default()}",
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

#[inline_props]
pub fn GithubDiscussionIcon<'a>(
    cx: Scope,
    github_discussion: Option<&'a GithubDiscussion>,
    class: Option<&'a str>,
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
            Some(GithubDiscussionStateReason::Duplicate) => render! {
                GithubDiscussionDuplicate { class: "{class} {duplicate_icon_style}" }
            },
            Some(GithubDiscussionStateReason::Outdated) => render! {
                GithubDiscussionOutdated { class: "{class} {outdated_icon_style}" }
            },
            Some(GithubDiscussionStateReason::Reopened) => render! {
                GithubDiscussionOpened { class: "{class} {opened_icon_style}" }
            },
            Some(GithubDiscussionStateReason::Resolved) => render! {
                GithubDiscussionClosed { class: "{class} {closed_icon_style}" }
            },
            _ => render! {
                GithubDiscussionOpened { class: "{class} {opened_icon_style}" }
            },
        };
    }

    render! { GithubDiscussionOpened { class: "{class} {opened_icon_style}" } }
}
