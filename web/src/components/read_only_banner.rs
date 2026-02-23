#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{Icon, icons::bs_icons::BsExclamationTriangle};

use crate::{route::Route, services::user_service::CONNECTED_USER};

#[component]
pub fn ReadOnlyBanner() -> Element {
    let is_read_only = CONNECTED_USER
        .read()
        .as_ref()
        .map(|ctx| ctx.subscription.is_read_only)
        .unwrap_or(false);

    if !is_read_only {
        return rsx! {};
    }

    rsx! {
        div {
            class: "alert alert-warning rounded-none flex items-center justify-between gap-4 px-4 py-2",
            role: "alert",

            div {
                class: "flex items-center gap-2",
                Icon { class: "min-w-5 h-5", icon: BsExclamationTriangle }
                span {
                    class: "text-sm",
                    "Your account is in read-only mode. Subscribe to regain full access."
                }
            }

            Link {
                class: "btn btn-warning btn-sm",
                to: Route::SubscriptionSettingsPage {},
                "Subscribe Now"
            }
        }
    }
}
