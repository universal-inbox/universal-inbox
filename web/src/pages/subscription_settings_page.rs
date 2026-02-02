#![allow(non_snake_case)]

use dioxus::prelude::*;
use log::debug;

use crate::components::subscription_card::SubscriptionCard;

pub fn SubscriptionSettingsPage() -> Element {
    debug!("Rendering subscription settings page");

    rsx! {
        div {
            class: "h-full mx-auto flex flex-row px-4",

            div {
                class: "flex flex-col h-full w-full overflow-y-auto scroll-y-auto gap-4 p-8",

                SubscriptionCard {}
            }
        }
    }
}
