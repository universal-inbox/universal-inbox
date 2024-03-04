#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsSlack, Icon};

use universal_inbox::{
    integration_connection::provider::IntegrationProviderKind, notification::NotificationMetadata,
    task::TaskMetadata,
};

use crate::components::integrations::{
    github::icons::Github, google_mail::icons::GoogleMail, linear::icons::Linear,
    todoist::icons::Todoist,
};

#[component]
pub fn Notion<'a>(cx: Scope, class: Option<&'a str>) -> Element {
    render! {img {
        class: "{class.unwrap_or_default()}",
        src: "images/notion-logo.svg"
    }}
}

#[component]
pub fn GoogleDocs<'a>(cx: Scope, class: Option<&'a str>) -> Element {
    render! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: "{class.unwrap_or_default()}",
            role: "img",
            "viewBox": "0 0 24 24",
            fill: "currentColor",
            title { "Google Docs" }
            path {
                d: "M14.727 6.727H14V0H4.91c-.905 0-1.637.732-1.637 1.636v20.728c0 .904.732 1.636 1.636 1.636h14.182c.904 0 1.636-.732 1.636-1.636V6.727h-6zm-.545 10.455H7.09v-1.364h7.09v1.364zm2.727-3.273H7.091v-1.364h9.818v1.364zm0-3.273H7.091V9.273h9.818v1.363zM14.727 6h6l-6-6v6z"
            }
        }
    }
}

#[component]
pub fn TickTick<'a>(cx: Scope, class: Option<&'a str>) -> Element {
    render! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: "{class.unwrap_or_default()}",
            "viewBox": "0 0 192 192",
            fill: "none",
            g { id: "bg", "stroke-width": "0" }
            g { id:"trace", "stroke-linecap": "round", "stroke-linejoin": "round" }
            g {
                id:"icon",
                path {
                    stroke: "currentColor",
                    "stroke-linecap": "round",
                    "stroke-width": "12",
                    d: "m69 87 23.94 20.394a4 4 0 0 0 5.652-.466L150 46"
                }
                path {
                    stroke: "currentColor",
                    "stroke-linecap": "round",
                    "stroke-linejoin": "round",
                    "stroke-width": "12",
                    d: "M170 96c0 40.869-33.131 74-74 74-40.87 0-74-33.131-74-74 0-40.87 33.13-74 74-74"
                }
            }
        }
    }
}

#[component]
pub fn IntegrationProviderIcon<'a>(
    cx: Scope,
    class: &'a str,
    provider_kind: IntegrationProviderKind,
) -> Element {
    // tag: New notification integration
    match provider_kind {
        IntegrationProviderKind::Github => render! { Github { class: class } },
        IntegrationProviderKind::Linear => render! { Linear { class: class } },
        IntegrationProviderKind::GoogleMail => render! { GoogleMail { class: class } },
        IntegrationProviderKind::Notion => render! { Notion { class: class } },
        IntegrationProviderKind::GoogleDocs => render! { GoogleDocs { class: class } },
        IntegrationProviderKind::Slack => render! { Icon { class: class, icon: BsSlack } },
        IntegrationProviderKind::Todoist => render! { Todoist { class: class } },
        IntegrationProviderKind::TickTick => render! { TickTick { class: class } },
    }
}

#[component]
pub fn NotificationMetadataIcon<'a>(
    cx: Scope,
    class: &'a str,
    notification_metadata: &'a NotificationMetadata,
) -> Element {
    // tag: New notification integration
    match notification_metadata {
        NotificationMetadata::Github(_) => render! { Github { class: class } },
        NotificationMetadata::Linear(_) => render! { Linear { class: class } },
        NotificationMetadata::GoogleMail(_) => render! { GoogleMail { class: class } },
        NotificationMetadata::Todoist => render! { Todoist { class: class } },
        NotificationMetadata::Slack(_) => render! { Icon { class: class, icon: BsSlack } },
    }
}

#[component]
pub fn TaskMetadataIcon<'a>(
    cx: Scope,
    class: &'a str,
    _task_metadata: &'a TaskMetadata,
) -> Element {
    // _task_metadata is not yet used as Todoist is the only task provider for now
    render! { Todoist { class: class } }
}
