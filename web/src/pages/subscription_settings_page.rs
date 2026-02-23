#![allow(non_snake_case)]

use dioxus::prelude::*;
use log::{debug, error};

use crate::{
    components::{
        subscription_card::SubscriptionCard,
        toast_zone::{Toast, ToastKind},
    },
    services::toast_service::ToastCommand,
    utils::{clean_url_query_params, current_location},
};

#[derive(Debug, Clone, Copy, PartialEq)]
enum CheckoutResult {
    Success,
    Canceled,
}

fn parse_subscription_query_param() -> Option<CheckoutResult> {
    let url = current_location().ok()?;
    let subscription_param = url
        .query_pairs()
        .find(|(key, _)| key == "subscription")
        .map(|(_, value)| value.to_string())?;

    match subscription_param.as_str() {
        "success" => Some(CheckoutResult::Success),
        "canceled" => Some(CheckoutResult::Canceled),
        _ => None,
    }
}

pub fn SubscriptionSettingsPage() -> Element {
    debug!("Rendering subscription settings page");
    let toast_service = use_coroutine_handle::<ToastCommand>();

    use_effect(move || {
        if let Some(checkout_result) = parse_subscription_query_param() {
            let toast = match checkout_result {
                CheckoutResult::Success => Toast {
                    kind: ToastKind::Success,
                    message: "Subscription activated successfully! Thank you for subscribing."
                        .to_string(),
                    timeout: Some(8_000),
                    ..Default::default()
                },
                CheckoutResult::Canceled => Toast {
                    kind: ToastKind::Message,
                    message: "Checkout was canceled. You can try again when you're ready."
                        .to_string(),
                    timeout: Some(8_000),
                    ..Default::default()
                },
            };
            toast_service.send(ToastCommand::Push(toast));

            if let Err(err) = clean_url_query_params() {
                error!("Failed to clean URL query params: {err}");
            }
        }
    });

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
