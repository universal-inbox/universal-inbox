#![allow(non_snake_case)]
use dioxus::prelude::*;

use universal_inbox::{
    notification::NotificationWithTask,
    third_party::integrations::api::{APISource, WebPage},
};

use crate::{
    components::{
        preview_card_header::PreviewCardHeader,
        ui::{Card, CardVariant, MetadataGrid, MetadataItem},
    },
    utils::format_elapsed_time,
};

#[component]
pub fn WebPagePreview(
    notification: ReadSignal<NotificationWithTask>,
    web_page: ReadSignal<WebPage>,
) -> Element {
    let page = web_page();
    let host = page
        .url
        .host_str()
        .map(|h| h.to_string())
        .unwrap_or_else(|| page.url.to_string());
    let elapsed = format_elapsed_time(page.timestamp);
    let captured_label = page.timestamp.format("%Y-%m-%d %H:%M UTC").to_string();
    let source_label = match &page.source {
        APISource::UniversalInboxExtension => "Universal Inbox extension".to_string(),
        APISource::Other(s) => s.clone(),
    };

    let brand_icon = if let Some(favicon) = page.favicon.as_ref() {
        rsx! {
            img {
                class: "h-4 w-4",
                src: "{favicon}",
                alt: ""
            }
        }
    } else {
        rsx! { span { class: "icon-[lucide--globe] size-4" } }
    };

    rsx! {
        div {
            class: "flex flex-col w-full h-full",

            PreviewCardHeader {
                brand_icon,
                title: page.title.clone(),
                subline: rsx! {
                    span { "Web page" }
                    span { class: "sep", "·" }
                    span { "{host}" }
                    span { class: "sep", "·" }
                    span { "{elapsed} ago" }
                }
            }

            div {
                id: "web-page-preview-details",
                class: "flex flex-col gap-2 w-full h-full overflow-y-auto scroll-y-auto p-3",

                Card {
                    variant: CardVariant::Default,
                    MetadataGrid {
                        MetadataItem {
                            label: "URL".to_string(),
                            value: rsx! {
                                a {
                                    href: "{page.url}",
                                    target: "_blank",
                                    rel: "noopener noreferrer",
                                    "{page.url}"
                                }
                            },
                        }
                        MetadataItem {
                            label: "Source".to_string(),
                            value: rsx! { "{source_label}" },
                        }
                        MetadataItem {
                            label: "Captured".to_string(),
                            value: rsx! { "{captured_label}" },
                        }
                    }
                }
            }
        }
    }
}
