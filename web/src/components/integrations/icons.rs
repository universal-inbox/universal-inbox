#![allow(non_snake_case)]

use cfg_if::cfg_if;
use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsSlack, Icon};

use universal_inbox::{
    integration_connection::provider::IntegrationProviderKind,
    notification::NotificationSourceKind, task::TaskSourceKind,
};

use crate::{
    components::integrations::{
        github::icons::Github, google_calendar::icons::GoogleCalendar,
        google_drive::icons::GoogleDrive, google_mail::icons::GoogleMail, linear::icons::Linear,
        todoist::icons::Todoist,
    },
    icons::UniversalInbox,
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
        IntegrationProviderKind::GoogleMail => rsx! { GoogleMail { class } },
        IntegrationProviderKind::GoogleDrive => rsx! { GoogleDrive { class } },
        IntegrationProviderKind::Notion => rsx! { Notion { class } },
        IntegrationProviderKind::Slack => rsx! { Icon { class, icon: BsSlack } },
        IntegrationProviderKind::Todoist => rsx! { Todoist { class } },
        IntegrationProviderKind::TickTick => rsx! { TickTick { class } },
        IntegrationProviderKind::API => rsx! { UniversalInbox { class } },
    }
}

#[component]
pub fn NotificationIcon(kind: NotificationSourceKind) -> Element {
    // tag: New notification integration
    match kind {
        NotificationSourceKind::Github => rsx! { Github { class: "h-5 w-5" } },
        NotificationSourceKind::Linear => rsx! { Linear { class: "h-5 w-5" } },
        NotificationSourceKind::GoogleCalendar => rsx! { GoogleCalendar { class: "h-8 w-8" } },
        NotificationSourceKind::GoogleDrive => rsx! { GoogleDrive { class: "h-5 w-5" } },
        NotificationSourceKind::GoogleMail => rsx! { GoogleMail { class: "h-5 w-5" } },
        NotificationSourceKind::Todoist => rsx! { Todoist { class: "h-5 w-5" } },
        NotificationSourceKind::Slack => rsx! { Icon { class: "h-5 w-5", icon: BsSlack } },
        NotificationSourceKind::API => rsx! { UniversalInbox { class: "h-5 w-5" } },
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
