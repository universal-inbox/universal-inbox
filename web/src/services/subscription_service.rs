#![allow(non_snake_case)]

use anyhow::Result;
use dioxus::prelude::*;
use futures_util::StreamExt;
use log::error;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    components::toast_zone::{Toast, ToastKind},
    model::UniversalInboxUIModel,
    services::{
        api::call_api,
        toast_service::{ToastCommand, ToastUpdate},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BillingInterval {
    Monthly,
    Annual,
}

#[derive(Debug, Serialize)]
pub struct CreateCheckoutSessionRequest {
    pub billing_interval: BillingInterval,
    pub success_url: String,
    pub cancel_url: String,
}

#[derive(Debug, Deserialize)]
pub struct CheckoutSessionResponse {
    pub checkout_url: String,
}

#[derive(Debug, Serialize)]
pub struct CreatePortalSessionRequest {
    pub return_url: String,
}

#[derive(Debug, Deserialize)]
pub struct PortalSessionResponse {
    pub portal_url: String,
}

#[derive(Debug)]
pub enum SubscriptionCommand {
    CreateCheckoutSession {
        billing_interval: BillingInterval,
        success_url: String,
        cancel_url: String,
    },
    OpenBillingPortal {
        return_url: String,
    },
}

pub async fn subscription_service(
    mut rx: UnboundedReceiver<SubscriptionCommand>,
    api_base_url: Url,
    ui_model: Signal<UniversalInboxUIModel>,
    toast_service: Coroutine<ToastCommand>,
) {
    loop {
        let msg = rx.next().await;
        match msg {
            Some(SubscriptionCommand::CreateCheckoutSession {
                billing_interval,
                success_url,
                cancel_url,
            }) => {
                let toast = Toast {
                    kind: ToastKind::Loading,
                    message: "Creating checkout session...".to_string(),
                    ..Default::default()
                };
                let toast_id = toast.id;
                toast_service.send(ToastCommand::Push(toast));

                let request = CreateCheckoutSessionRequest {
                    billing_interval,
                    success_url,
                    cancel_url,
                };

                let result: Result<CheckoutSessionResponse> = call_api(
                    Method::POST,
                    &api_base_url,
                    "subscriptions/checkout",
                    Some(request),
                    Some(ui_model),
                )
                .await;

                match result {
                    Ok(response) => {
                        toast_service.send(ToastCommand::Update(ToastUpdate {
                            id: toast_id,
                            kind: Some(ToastKind::Success),
                            message: Some("Redirecting to checkout...".to_string()),
                            timeout: Some(Some(3_000)),
                        }));
                        if let Err(err) = redirect_to(&response.checkout_url) {
                            error!("Failed to redirect to checkout: {err}");
                        }
                    }
                    Err(err) => {
                        error!("Failed to create checkout session: {err}");
                        toast_service.send(ToastCommand::Update(ToastUpdate {
                            id: toast_id,
                            kind: Some(ToastKind::Failure),
                            message: Some(
                                "Failed to create checkout session. Please try again.".to_string(),
                            ),
                            timeout: Some(Some(10_000)),
                        }));
                    }
                }
            }
            Some(SubscriptionCommand::OpenBillingPortal { return_url }) => {
                let toast = Toast {
                    kind: ToastKind::Loading,
                    message: "Opening billing portal...".to_string(),
                    ..Default::default()
                };
                let toast_id = toast.id;
                toast_service.send(ToastCommand::Push(toast));

                let request = CreatePortalSessionRequest { return_url };

                let result: Result<PortalSessionResponse> = call_api(
                    Method::POST,
                    &api_base_url,
                    "subscriptions/portal",
                    Some(request),
                    Some(ui_model),
                )
                .await;

                match result {
                    Ok(response) => {
                        toast_service.send(ToastCommand::Update(ToastUpdate {
                            id: toast_id,
                            kind: Some(ToastKind::Success),
                            message: Some("Redirecting to billing portal...".to_string()),
                            timeout: Some(Some(3_000)),
                        }));
                        if let Err(err) = redirect_to(&response.portal_url) {
                            error!("Failed to redirect to billing portal: {err}");
                        }
                    }
                    Err(err) => {
                        error!("Failed to open billing portal: {err}");
                        toast_service.send(ToastCommand::Update(ToastUpdate {
                            id: toast_id,
                            kind: Some(ToastKind::Failure),
                            message: Some(
                                "Failed to open billing portal. Please try again.".to_string(),
                            ),
                            timeout: Some(Some(10_000)),
                        }));
                    }
                }
            }
            None => {}
        }
    }
}

fn redirect_to(url: &str) -> Result<(), String> {
    let window = web_sys::window().ok_or("No window object")?;
    window
        .location()
        .set_href(url)
        .map_err(|e| format!("Failed to set location: {:?}", e))
}
