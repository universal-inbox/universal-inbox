#![allow(non_snake_case)]
use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{BsArrowUpRightSquare, BsLink45deg},
    Icon,
};

use universal_inbox::{
    notification::NotificationWithTask, third_party::integrations::api::WebPage,
};

#[component]
pub fn WebPagePreview(
    notification: ReadOnlySignal<NotificationWithTask>,
    web_page: ReadOnlySignal<WebPage>,
) -> Element {
    let source_icon = if web_page().favicon.is_some() {
        rsx! {
            img {
                class: "h-5 w-5 min-w-5",
                src: "{web_page().favicon.as_ref().unwrap()}",
                alt: "Favicon"
            }
        }
    } else {
        rsx! { Icon { class: "h-5 w-5 min-w-5", icon: BsLink45deg } }
    };

    rsx! {
        div {
            class: "flex flex-col gap-2 w-full h-full",

            h3 {
                class: "flex items-center gap-2 text-base",

                { source_icon }
                a {
                    class: "flex items-center",
                    href: "{web_page().url}",
                    target: "_blank",
                    span { "{web_page().title}" }
                    Icon { class: "h-5 w-5 min-w-5 text-base-content/50 p-1", icon: BsArrowUpRightSquare }
                }
            }

            div {
                id: "web-page-preview-details",
                class: "flex flex-col gap-2 w-full h-full overflow-y-auto scroll-y-auto",

                div {
                    class: "flex text-base-content/50 gap-1 text-xs",

                    span { "{web_page().url}" }
                }
            }
        }
    }
}
