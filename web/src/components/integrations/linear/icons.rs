#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::third_party::integrations::linear::{
    LinearIssue, LinearProject, LinearProjectState, LinearWorkflowStateType,
};

use crate::theme::{
    BACKLOG_TEXT_COLOR_CLASS, CANCELED_TEXT_COLOR_CLASS, COMPLETED_TEXT_COLOR_CLASS,
    DRAFT_TEXT_COLOR_CLASS, STARTED_TEXT_COLOR_CLASS,
};

#[component]
pub fn Linear(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: class.unwrap_or_default(),
            role: "img",
            "viewBox": "0 0 100 100",
            fill: "currentColor",
            stroke: "currentColor",
            title { "Linear" }
            path {
                d: "M1.22541 61.5228c-.2225-.9485.90748-1.5459 1.59638-.857L39.3342 97.1782c.6889.6889.0915 1.8189-.857 1.5964C20.0515 94.4522 5.54779 79.9485 1.22541 61.5228ZM.00189135 46.8891c-.01764375.2833.08887215.5599.28957165.7606L52.3503 99.7085c.2007.2007.4773.3075.7606.2896 2.3692-.1476 4.6938-.46 6.9624-.9259.7645-.157 1.0301-1.0963.4782-1.6481L2.57595 39.4485c-.55186-.5519-1.49117-.2863-1.648174.4782-.465915 2.2686-.77832 4.5932-.92588465 6.9624ZM4.21093 29.7054c-.16649.3738-.08169.8106.20765 1.1l64.77602 64.776c.2894.2894.7262.3742 1.1.2077 1.7861-.7956 3.5171-1.6927 5.1855-2.684.5521-.328.6373-1.0867.1832-1.5407L8.43566 24.3367c-.45409-.4541-1.21271-.3689-1.54074.1832-.99132 1.6684-1.88843 3.3994-2.68399 5.1855ZM12.6587 18.074c-.3701-.3701-.393-.9637-.0443-1.3541C21.7795 6.45931 35.1114 0 49.9519 0 77.5927 0 100 22.4073 100 50.0481c0 14.8405-6.4593 28.1724-16.7199 37.3375-.3903.3487-.984.3258-1.3542-.0443L12.6587 18.074Z"
            }
        }
    }
}

#[component]
pub fn LinearIssueIcon(
    linear_issue: ReadOnlySignal<LinearIssue>,
    class: Option<String>,
) -> Element {
    let class = class.unwrap_or_default();

    let (icon, color_style) = match linear_issue().state.r#type {
        LinearWorkflowStateType::Triage => (
            rsx! { LinearIssueTriageIcon { class: "{class}" } },
            DRAFT_TEXT_COLOR_CLASS,
        ),
        LinearWorkflowStateType::Backlog => (
            rsx! { LinearIssueBacklogIcon { class: "{class}" } },
            BACKLOG_TEXT_COLOR_CLASS,
        ),
        LinearWorkflowStateType::Unstarted => (
            rsx! { LinearIssueUnstartedIcon { class: "{class}" } },
            BACKLOG_TEXT_COLOR_CLASS,
        ),
        LinearWorkflowStateType::Started => (
            rsx! { LinearIssueStartedIcon { class: "{class}" } },
            STARTED_TEXT_COLOR_CLASS,
        ),
        LinearWorkflowStateType::Completed => (
            rsx! { LinearIssueCompletedIcon { class: "{class}" } },
            COMPLETED_TEXT_COLOR_CLASS,
        ),
        LinearWorkflowStateType::Canceled => (
            rsx! { LinearIssueCanceledIcon { class: "{class}" } },
            CANCELED_TEXT_COLOR_CLASS,
        ),
    };

    rsx! {
        div { class: "{color_style}", { icon } }
    }
}

#[component]
pub fn LinearIssueTriageIcon(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: class.unwrap_or_default(),
            role: "img",
            "viewBox": "0 0 14 14",
            fill: "currentColor",
            title { "Linear issue in triage" }
            path {
                d: "M7 14C10.866 14 14 10.866 14 7C14 3.13403 10.866 0 7 0C3.134 0 0 3.13403 0 7C0 10.866 3.134 14 7 14ZM8.0126 9.50781V7.98224H5.9874V9.50787C5.9874 9.92908 5.4767 10.1549 5.14897 9.8786L2.17419 7.37073C1.94194 7.17493 1.94194 6.82513 2.17419 6.62933L5.14897 4.12146C5.4767 3.84515 5.9874 4.07098 5.9874 4.49219V6.01764H8.0126V4.49213C8.0126 4.07092 8.5233 3.84509 8.85103 4.1214L11.8258 6.62927C12.0581 6.82507 12.0581 7.17487 11.8258 7.37067L8.85103 9.87854C8.5233 10.1548 8.0126 9.92902 8.0126 9.50781Z"
            }
        }
    }
}

#[component]
pub fn LinearIssueBacklogIcon(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: class.unwrap_or_default(),
            role: "img",
            "viewBox": "0 0 14 14",
            fill: "currentColor",
            title { "Linear issue in backlog" }
            path {
                stroke: "none",
                d: "M13.9408 7.91426L11.9576 7.65557C11.9855 7.4419 12 7.22314 12 7C12 6.77686 11.9855 6.5581 11.9576 6.34443L13.9408 6.08573C13.9799 6.38496 14 6.69013 14 7C14 7.30987 13.9799 7.61504 13.9408 7.91426ZM13.4688 4.32049C13.2328 3.7514 12.9239 3.22019 12.5538 2.73851L10.968 3.95716C11.2328 4.30185 11.4533 4.68119 11.6214 5.08659L13.4688 4.32049ZM11.2615 1.4462L10.0428 3.03204C9.69815 2.76716 9.31881 2.54673 8.91341 2.37862L9.67951 0.531163C10.2486 0.767153 10.7798 1.07605 11.2615 1.4462ZM7.91426 0.0591659L7.65557 2.04237C7.4419 2.01449 7.22314 2 7 2C6.77686 2 6.5581 2.01449 6.34443 2.04237L6.08574 0.059166C6.38496 0.0201343 6.69013 0 7 0C7.30987 0 7.61504 0.0201343 7.91426 0.0591659ZM4.32049 0.531164L5.08659 2.37862C4.68119 2.54673 4.30185 2.76716 3.95716 3.03204L2.73851 1.4462C3.22019 1.07605 3.7514 0.767153 4.32049 0.531164ZM1.4462 2.73851L3.03204 3.95716C2.76716 4.30185 2.54673 4.68119 2.37862 5.08659L0.531164 4.32049C0.767153 3.7514 1.07605 3.22019 1.4462 2.73851ZM0.0591659 6.08574C0.0201343 6.38496 0 6.69013 0 7C0 7.30987 0.0201343 7.61504 0.059166 7.91426L2.04237 7.65557C2.01449 7.4419 2 7.22314 2 7C2 6.77686 2.01449 6.5581 2.04237 6.34443L0.0591659 6.08574ZM0.531164 9.67951L2.37862 8.91341C2.54673 9.31881 2.76716 9.69815 3.03204 10.0428L1.4462 11.2615C1.07605 10.7798 0.767153 10.2486 0.531164 9.67951ZM2.73851 12.5538L3.95716 10.968C4.30185 11.2328 4.68119 11.4533 5.08659 11.6214L4.32049 13.4688C3.7514 13.2328 3.22019 12.9239 2.73851 12.5538ZM6.08574 13.9408L6.34443 11.9576C6.5581 11.9855 6.77686 12 7 12C7.22314 12 7.4419 11.9855 7.65557 11.9576L7.91427 13.9408C7.61504 13.9799 7.30987 14 7 14C6.69013 14 6.38496 13.9799 6.08574 13.9408ZM9.67951 13.4688L8.91341 11.6214C9.31881 11.4533 9.69815 11.2328 10.0428 10.968L11.2615 12.5538C10.7798 12.9239 10.2486 13.2328 9.67951 13.4688ZM12.5538 11.2615L10.968 10.0428C11.2328 9.69815 11.4533 9.31881 11.6214 8.91341L13.4688 9.67951C13.2328 10.2486 12.924 10.7798 12.5538 11.2615Z"
            }
        }
    }
}

#[component]
pub fn LinearIssueCompletedIcon(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: class.unwrap_or_default(),
            role: "img",
            "viewBox": "0 0 14 14",
            fill: "currentColor",
            title { "completed Linear issue" }
            path {
                "fill-rule": "evenodd",
                "clip-rule": "evenodd",
                d: "M7 0C3.13401 0 0 3.13401 0 7C0 10.866 3.13401 14 7 14C10.866 14 14 10.866 14 7C14 3.13401 10.866 0 7 0ZM11.101 5.10104C11.433 4.76909 11.433 4.23091 11.101 3.89896C10.7691 3.56701 10.2309 3.56701 9.89896 3.89896L5.5 8.29792L4.10104 6.89896C3.7691 6.56701 3.2309 6.56701 2.89896 6.89896C2.56701 7.2309 2.56701 7.7691 2.89896 8.10104L4.89896 10.101C5.2309 10.433 5.7691 10.433 6.10104 10.101L11.101 5.10104Z"
            }
        }
    }
}

#[component]
pub fn LinearIssueCanceledIcon(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: class.unwrap_or_default(),
            role: "img",
            "viewBox": "0 0 14 14",
            fill: "currentColor",
            title { "canceled Linear issue" }
            path {
                "fill-rule": "evenodd",
                "clip-rule": "evenodd",
                d: "M7 14C10.866 14 14 10.866 14 7C14 3.13401 10.866 0 7 0C3.13401 0 0 3.13401 0 7C0 10.866 3.13401 14 7 14ZM5.03033 3.96967C4.73744 3.67678 4.26256 3.67678 3.96967 3.96967C3.67678 4.26256 3.67678 4.73744 3.96967 5.03033L5.93934 7L3.96967 8.96967C3.67678 9.26256 3.67678 9.73744 3.96967 10.0303C4.26256 10.3232 4.73744 10.3232 5.03033 10.0303L7 8.06066L8.96967 10.0303C9.26256 10.3232 9.73744 10.3232 10.0303 10.0303C10.3232 9.73744 10.3232 9.26256 10.0303 8.96967L8.06066 7L10.0303 5.03033C10.3232 4.73744 10.3232 4.26256 10.0303 3.96967C9.73744 3.67678 9.26256 3.67678 8.96967 3.96967L7 5.93934L5.03033 3.96967Z"
            }
        }
    }
}

#[component]
pub fn LinearIssueUnstartedIcon(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: class.unwrap_or_default(),
            role: "img",
            "viewBox": "0 0 14 14",
            fill: "none",
            title { "Linear issue to do" }
            rect { x: "1", y: "1", width: "12", height: "12", rx: "6", stroke: "currentColor", "stroke-width": "2", fill: "none" }
            path {
                fill: "currentColor",
                stroke: "none",
                transform: "translate(3.5,3.5)",
                d: "M 3.5,3.5 L3.5,0 A3.5,3.5 0 0,1 3.5, 0 z"
            }
        }
    }
}

#[component]
pub fn LinearIssueStartedIcon(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: class.unwrap_or_default(),
            role: "img",
            "viewBox": "0 0 14 14",
            fill: "none",
            title { "In progress Linear issue" }
            rect { x: "1", y: "1", width: "12", height: "12", rx: "6", stroke: "currentColor", "stroke-width": "2", fill: "none" }
            path {
                fill: "currentColor",
                stroke: "none",
                transform: "translate(3.5,3.5)",
                d: "M 3.5,3.5 L3.5,0 A3.5,3.5 0 0,1 3.5, 7 z"
            }
        }
    }
}

#[component]
pub fn LinearProjectIcon(
    linear_project: ReadOnlySignal<LinearProject>,
    class: Option<String>,
) -> Element {
    let class = class.unwrap_or_default();

    let (icon, color_style) = match linear_project().state {
        LinearProjectState::Planned => (
            rsx! { LinearProjectPlannedIcon { class: "{class}" } },
            DRAFT_TEXT_COLOR_CLASS,
        ),
        LinearProjectState::Backlog => (
            rsx! { LinearProjectBacklogIcon { class: "{class}" } },
            BACKLOG_TEXT_COLOR_CLASS,
        ),
        LinearProjectState::Started => (
            rsx! { LinearProjectStartedIcon { class: "{class}" } },
            STARTED_TEXT_COLOR_CLASS,
        ),
        LinearProjectState::Paused => (
            rsx! { LinearProjectPausedIcon { class: "{class}" } },
            BACKLOG_TEXT_COLOR_CLASS,
        ),
        LinearProjectState::Completed => (
            rsx! { LinearProjectCompletedIcon { class: "{class}" } },
            COMPLETED_TEXT_COLOR_CLASS,
        ),
        LinearProjectState::Canceled => (
            rsx! { LinearProjectCanceledIcon { class: "{class}" } },
            CANCELED_TEXT_COLOR_CLASS,
        ),
    };

    rsx! {
        div { class: "{color_style}", { icon } }
    }
}

#[component]
pub fn LinearProjectPlannedIcon(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: class.unwrap_or_default(),
            role: "img",
            "viewBox": "1 1 14 14",
            fill: "currentColor",
            title { "planned Linear project" }
            path {
                "fill-rule": "evenodd",
                "clip-rule": "evenodd",
                d: "M12.5 5.36133L8 2.73633L3.5 5.36133L3.5 10.6382L8 13.2632L12.5 10.6382L12.5 5.36133ZM8.75581 1.44066C8.28876 1.16822 7.71124 1.16822 7.24419 1.44066L2.74419 4.06566C2.28337 4.33448 2 4.82783 2 5.36133V10.6382C2 11.1717 2.28337 11.6651 2.74419 11.9339L7.24419 14.5589C7.71124 14.8313 8.28876 14.8313 8.75581 14.5589L13.2558 11.9339C13.7166 11.6651 14 11.1717 14 10.6382V5.36133C14 4.82783 13.7166 4.33448 13.2558 4.06566L8.75581 1.44066Z"
            }
        }
    }
}

#[component]
pub fn LinearProjectBacklogIcon(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: class.unwrap_or_default(),
            role: "img",
            "viewBox": "1 1 14 14",
            fill: "currentColor",
            title { "Linear project in backlog" }
            path {
                d: "M2 4.74695C2 4.68722 2.01039 4.62899 2.02989 4.57451L2.11601 4.42269C2.15266 4.37819 2.19711 4.33975 2.24806 4.30966L3.16473 3.76824L3.92054 5.08013L3.5 5.32852V5.8313H2V4.74695Z" }
            path {
                d: "M4.8372 4.53871L4.0814 3.22682L5.91473 2.14398L6.67054 3.45588L4.8372 4.53871Z"
            }
            path {
                d: "M7.5872 2.91446L6.8314 1.60257L7.74806 1.06115C7.7997 1.03065 7.85539 1.01027 7.91244 1H8.08756C8.14461 1.01027 8.2003 1.03065 8.25194 1.06115L9.1686 1.60257L8.4128 2.91446L8 2.67065L7.5872 2.91446Z"
            }
            path {
                d: "M9.32946 3.45588L10.0853 2.14398L11.9186 3.22682L11.1628 4.53871L9.32946 3.45588Z"
            }
            path {
                d: "M12.0795 5.08013L12.8353 3.76824L13.7519 4.30966C13.8029 4.33975 13.8473 4.37819 13.884 4.42269L13.9701 4.57451C13.9896 4.62899 14 4.68722 14 4.74695V5.8313H12.5V5.32852L12.0795 5.08013Z"
            }
            path {
                d: "M12.5 6.91565H14V9.08435H12.5V6.91565Z"
            }
            path {
                d: "M12.5 10.1687H14V11.253C14 11.3128 13.9896 11.371 13.9701 11.4255L13.884 11.5773C13.8473 11.6218 13.8029 11.6602 13.7519 11.6903L12.8353 12.2318L12.0795 10.9199L12.5 10.6715V10.1687Z"
            }
            path {
                d: "M11.1628 11.4613L11.9186 12.7732L10.0853 13.856L9.32946 12.5441L11.1628 11.4613Z"
            }
            path {
                d: "M8.4128 13.0855L9.1686 14.3974L8.25194 14.9389C8.2003 14.9694 8.14461 14.9897 8.08756 15H7.91244C7.85539 14.9897 7.7997 14.9694 7.74806 14.9389L6.8314 14.3974L7.5872 13.0855L8 13.3294L8.4128 13.0855Z"
            }
            path {
                d: "M6.67054 12.5441L5.91473 13.856L4.0814 12.7732L4.8372 11.4613L6.67054 12.5441Z"
            }
            path {
                d: "M3.92054 10.9199L3.16473 12.2318L2.24806 11.6903C2.19711 11.6602 2.15266 11.6218 2.11601 11.5773L2.02989 11.4255C2.01039 11.371 2 11.3128 2 11.253V10.1687H3.5V10.6715L3.92054 10.9199Z"
            }
            path {
                d: "M3.5 9.08435H2V6.91565H3.5V9.08435Z"
            }
        }
    }
}

#[component]
pub fn LinearProjectCompletedIcon(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: class.unwrap_or_default(),
            role: "img",
            "viewBox": "1 1 14 14",
            fill: "currentColor",
            title { "completed Linear project" }
            path {
                "fill-rule": "evenodd",
                "clip-rule": "evenodd",
                d: "M12.5 5.125L8 2.5L3.5 5.125L3.5 10.4019L8 13.0269L12.5 10.4019L12.5 5.125ZM8.75581 1.20433C8.28876 0.93189 7.71124 0.931889 7.24419 1.20433L2.74419 3.82933C2.28337 4.09815 2 4.5915 2 5.125V10.4019C2 10.9354 2.28337 11.4287 2.74419 11.6976L7.24419 14.3226C7.71124 14.595 8.28876 14.595 8.75581 14.3226L13.2558 11.6976C13.7166 11.4287 14 10.9354 14 10.4019V5.125C14 4.5915 13.7166 4.09815 13.2558 3.82933L8.75581 1.20433Z"
            }
            path {
                "fill-rule": "evenodd",
                "clip-rule": "evenodd",
                d: "M10.7381 5.69424C11.0526 5.96381 11.089 6.43728 10.8194 6.75178L7.81944 10.2518C7.68349 10.4104 7.48754 10.5051 7.27878 10.5131C7.07003 10.5212 6.86739 10.4417 6.71967 10.294L5.21967 8.79402C4.92678 8.50112 4.92678 8.02625 5.21967 7.73336C5.51256 7.44046 5.98744 7.44046 6.28033 7.73336L7.20764 8.66066L9.68056 5.77559C9.95012 5.4611 10.4236 5.42468 10.7381 5.69424Z"
            }
        }
    }
}

#[component]
pub fn LinearProjectCanceledIcon(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: class.unwrap_or_default(),
            role: "img",
            "viewBox": "1 1 14 14",
            fill: "currentColor",
            title { "canceled Linear project" }
            path {
                d: "M5.96967 5.96967C6.26256 5.67678 6.73744 5.67678 7.03033 5.96967L8 6.93934L8.96967 5.96967C9.26256 5.67678 9.73744 5.67678 10.0303 5.96967C10.3232 6.26256 10.3232 6.73744 10.0303 7.03033L9.06066 8L10.0303 8.96967C10.3232 9.26256 10.3232 9.73744 10.0303 10.0303C9.73744 10.3232 9.26256 10.3232 8.96967 10.0303L8 9.06066L7.03033 10.0303C6.73744 10.3232 6.26256 10.3232 5.96967 10.0303C5.67678 9.73744 5.67678 9.26256 5.96967 8.96967L6.93934 8L5.96967 7.03033C5.67678 6.73744 5.67678 6.26256 5.96967 5.96967Z"
            }
            path {
                "fill-rule": "evenodd",
                "clip-rule": "evenodd",
                d: "M8.75581 1.21148C8.28876 0.929507 7.71124 0.929507 7.24419 1.21148L2.74419 3.92829C2.28337 4.20651 2 4.71711 2 5.26927V10.7307C2 11.2829 2.28337 11.7935 2.74419 12.0717L7.24419 14.7885C7.71124 15.0705 8.28876 15.0705 8.75581 14.7885L13.2558 12.0717C13.7166 11.7935 14 11.2829 14 10.7307V5.26927C14 4.71711 13.7166 4.20651 13.2558 3.92829L8.75581 1.21148ZM12.5 5.26928L8 2.55246L3.5 5.26927L3.5 10.7307L8 13.4475L12.5 10.7307L12.5 5.26928Z"
            }
        }
    }
}

#[component]
pub fn LinearProjectPausedIcon(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: class.unwrap_or_default(),
            role: "img",
            "viewBox": "1 1 14 14",
            fill: "currentColor",
            title { "paused Linear project" }
            path {
                "fill-rule": "evenodd",
                "clip-rule": "evenodd",
                d: "M8.75581 1.21148C8.28876 0.929507 7.71124 0.929507 7.24419 1.21148L2.74419 3.92829C2.28337 4.20651 2 4.71711 2 5.26927V10.7307C2 11.2829 2.28337 11.7935 2.74419 12.0717L7.24419 14.7885C7.71124 15.0705 8.28876 15.0705 8.75581 14.7885L13.2558 12.0717C13.7166 11.7935 14 11.2829 14 10.7307V5.26927C14 4.71711 13.7166 4.20651 13.2558 3.92829L8.75581 1.21148ZM12.5 5.26928L8 2.55246L3.5 5.26927L3.5 10.7307L8 13.4475L12.5 10.7307L12.5 5.26928Z"
            }
            path {
                "fill-rule": "evenodd",
                "clip-rule": "evenodd",
                d: "M6.5 5.75C6.91421 5.75 7.25 6.08579 7.25 6.5V9.5C7.25 9.91421 6.91421 10.25 6.5 10.25C6.08579 10.25 5.75 9.91421 5.75 9.5V6.5C5.75 6.08579 6.08579 5.75 6.5 5.75Z"
            }
            path {
                "fill-rule": "evenodd",
                "clip-rule": "evenodd",
                d: "M9.5 5.75C9.91421 5.75 10.25 6.08579 10.25 6.5V9.5C10.25 9.91421 9.91421 10.25 9.5 10.25C9.08579 10.25 8.75 9.91421 8.75 9.5V6.5C8.75 6.08579 9.08579 5.75 9.5 5.75Z"
            }
        }
    }
}

#[component]
pub fn LinearProjectStartedIcon(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: class.unwrap_or_default(),
            "viewBox": "1 1 14 14",
            fill: "currentColor",
            title { "In progress Linear project" }
            path {
                d: "M8.3779 4.74233C8.14438 4.60607 7.85562 4.60607 7.6221 4.74233L5.37209 6.05513C5.14168 6.18957 5 6.4363 5 6.70311V9.34216C5 9.60897 5.14168 9.85573 5.37209 9.99016L7.6221 11.303C7.85562 11.4392 8.14438 11.4392 8.3779 11.303L10.6279 9.99016C10.8583 9.85573 11 9.60897 11 9.34216V6.70311C11 6.4363 10.8583 6.18957 10.6279 6.05513L8.3779 4.74233Z",
                mask: "url(#hole-50)"
            }
            mask {
                id: "hole-50",
                rect {
                    width: "100%",
                    height: "100%",
                    fill: "white"
                }
                circle {
                    r: "4",
                    cx: "7.5",
                    cy: "8",
                    fill: "black",
                    stroke: "white",
                    "stroke-width": "8",
                    "stroke-dasharray": "calc(12.56) 25.12",
                    transform: "rotate(-90) translate(-16)"
                }
            }
            path {
                "fill-rule": "evenodd",
                "clip-rule": "evenodd",
                d: "M7.24419 1.44066C7.71124 1.16822 8.28876 1.16822 8.75581 1.44066L13.2558 4.06566C13.7166 4.33448 14 4.82783 14 5.36133V10.6382C14 11.1717 13.7166 11.6651 13.2558 11.9339L8.75581 14.5589C8.28876 14.8313 7.71124 14.8313 7.24419 14.5589L2.74419 11.9339C2.28337 11.6651 2 11.1717 2 10.6382V5.36133C2 4.82783 2.28337 4.33448 2.74419 4.06566L7.24419 1.44066ZM8 2.73633L12.5 5.36133V10.6382L8 13.2632L3.5 10.6382V5.36133L8 2.73633Z"
            }
        }
    }
}

#[component]
pub fn LinearProjectMilestoneIcon(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: class.unwrap_or_default(),
            "viewBox": "0 0 16 16",
            fill: "none",
            title { "Linear project milestone" }
            path {
                d: "M7.3406 2.32C7.68741 1.89333 8.31259 1.89333 8.6594 2.32L12.7903 7.402C13.0699 7.74597 13.0699 8.25403 12.7903 8.598L8.6594 13.68C8.31259 14.1067 7.68741 14.1067 7.3406 13.68L3.2097 8.598C2.9301 8.25403 2.9301 7.74597 3.2097 7.402L7.3406 2.32Z",
                stroke: "currentColor",
                "stroke-width": "2",
                "stroke-linejoin": "round",
                style: "opacity: 0.4;"
            }
        }
    }
}

#[component]
pub fn LinearProjectHealtIcon(class: Option<String>) -> Element {
    rsx! {
        svg {
            "viewBox": "0 0 16 16",
            fill: "none",
            class: class.unwrap_or_default(),
            path {
                stroke: "currentColor",
                fill: "none",
                "stroke-width": "1.75",
                "stroke-linecap": "round",
                "stroke-linejoin": "round",
                d: "M1 9H4L6 13L10 3L12 7H15"
            }
        }
    }
}

#[component]
pub fn LinearProjectDefaultIcon(class: Option<String>) -> Element {
    rsx! {
        svg {
            class: class.unwrap_or_default(),
             style: "--icon-color: #DCD8FE93;",
            "viewBox": "0 0 16 16",
            fill: "currentColor",
            role: "img",
            "focusable": "false",
            "aria-hidden": "true",
            path {
                d: "M5.948 2H2.623A.623.623 0 0 0 2 2.623v3.325c0 .344.28.623.623.623h3.325c.344 0 .623-.279.623-.623V2.623A.623.623 0 0 0 5.948 2ZM13.377 2h-3.325a.623.623 0 0 0-.623.623v3.325c0 .344.279.623.623.623h3.325c.344 0 .623-.279.623-.623V2.623A.623.623 0 0 0 13.377 2ZM5.948 9.429H2.623a.623.623 0 0 0-.623.623v3.325c0 .344.28.623.623.623h3.325c.344 0 .623-.28.623-.623v-3.325a.623.623 0 0 0-.623-.623ZM13.377 9.429h-3.325a.623.623 0 0 0-.623.623v3.325c0 .344.279.623.623.623h3.325c.344 0 .623-.28.623-.623v-3.325a.623.623 0 0 0-.623-.623Z"

            }
        }
    }
}
