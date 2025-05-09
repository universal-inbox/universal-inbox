#![allow(non_snake_case)]

use cfg_if::cfg_if;
use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsSlack, Icon};

use universal_inbox::{
    integration_connection::provider::IntegrationProviderKind,
    notification::NotificationSourceKind, task::TaskSourceKind,
};

use crate::components::integrations::{
    github::icons::Github, google_calendar::icons::GoogleCalendar, google_mail::icons::GoogleMail,
    linear::icons::Linear, todoist::icons::Todoist,
};

cfg_if! {
    if #[cfg(feature = "trunk")] {
        const NOTION_LOGO: &str = "/images/notion-logo.svg";
    } else {
        const NOTION_LOGO: Asset = asset!("/images/notion-logo.svg");
    }
}

#[component]
pub fn Notion(class: Option<String>) -> Element {
    rsx! {
        img {
            class: class.unwrap_or_default(),
            src: "{NOTION_LOGO}",
        }
    }
}

#[component]
pub fn GoogleDocs(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: class.unwrap_or_default(),
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
pub fn TickTick(class: Option<String>) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            class: class.unwrap_or_default(),
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
pub fn IntegrationProviderIcon(class: String, provider_kind: IntegrationProviderKind) -> Element {
    // tag: New notification integration
    match provider_kind {
        IntegrationProviderKind::Github => rsx! { Github { class } },
        IntegrationProviderKind::Linear => rsx! { Linear { class } },
        IntegrationProviderKind::GoogleCalendar => rsx! { GoogleCalendar { class } },
        IntegrationProviderKind::GoogleDocs => rsx! { GoogleDocs { class } },
        IntegrationProviderKind::GoogleMail => rsx! { GoogleMail { class } },
        IntegrationProviderKind::Notion => rsx! { Notion { class } },
        IntegrationProviderKind::Slack => rsx! { Icon { class, icon: BsSlack } },
        IntegrationProviderKind::Todoist => rsx! { Todoist { class } },
        IntegrationProviderKind::TickTick => rsx! { TickTick { class } },
    }
}

#[component]
pub fn NotificationIcon(kind: NotificationSourceKind) -> Element {
    // tag: New notification integration
    match kind {
        NotificationSourceKind::Github => rsx! { Github { class: "h-5 w-5" } },
        NotificationSourceKind::Linear => rsx! { Linear { class: "h-5 w-5" } },
        NotificationSourceKind::GoogleCalendar => rsx! { GoogleCalendar { class: "h-8 w-8" } },
        NotificationSourceKind::GoogleMail => rsx! { GoogleMail { class: "h-5 w-5" } },
        NotificationSourceKind::Todoist => rsx! { Todoist { class: "h-5 w-5" } },
        NotificationSourceKind::Slack => rsx! { Icon { class: "h-5 w-5", icon: BsSlack } },
    }
}

#[component]
pub fn TaskIcon(class: String, kind: TaskSourceKind) -> Element {
    // tag: New notification integration
    match kind {
        TaskSourceKind::Todoist => rsx! { Todoist { class } },
        TaskSourceKind::Linear => rsx! { Linear { class } },
        TaskSourceKind::Slack => rsx! { Icon { class, icon: BsSlack } },
    }
}
