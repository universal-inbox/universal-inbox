#![allow(non_snake_case)]

use dioxus::prelude::*;

use crate::{
    components::ui::{Card, CardBody, CardHeader, CardMeta, CardVariant},
    config::APP_CONFIG,
};

#[component]
pub fn ExportDataCard() -> Element {
    let Some(api_base_url) = APP_CONFIG
        .read()
        .as_ref()
        .map(|config| config.api_base_url.clone())
    else {
        return rsx! {};
    };
    // A plain link: the browser sends the session cookie and saves the
    // `Content-Disposition: attachment` response straight to disk.
    let export_url = api_base_url
        .join("users/me/export")
        .map(|url| url.to_string())
        .unwrap_or_default();

    rsx! {
        section {
            role: "region",
            aria_label: "Export my data",

            Card { variant: CardVariant::ApiKeys, id: "export-data-card".to_string(),
                CardHeader { interactive: false,
                    span { class: "icon-[lucide--download] size-5" }
                    CardMeta { name: "Export my data" }
                }

                CardBody { class: "px-3.5 pb-3.5 flex flex-col gap-3",
                    p { class: "text-sm text-ui-base-muted leading-normal m-0",
                        "Download a JSON file with all the data Universal Inbox stores about you: "
                        "your profile and preferences, sign-in methods, API keys, authorized apps, "
                        "integration connections, notifications and tasks. "
                        "Secrets such as passwords and access tokens are never included."
                    }

                    div {
                        a {
                            id: "export-data-link",
                            class: "btn btn-soft btn-xs",
                            href: "{export_url}",
                            download: "",
                            span { class: "icon-[lucide--download]" }
                            "Export my data"
                        }
                    }
                }
            }
        }
    }
}
