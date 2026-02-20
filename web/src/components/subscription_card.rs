#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::bs_icons::{
        BsCheck, BsCreditCard, BsExclamationTriangle, BsHourglass, BsInfinity, BsXCircle,
    },
};

use universal_inbox::subscription::SubscriptionStatus;

use crate::{
    components::loading::Loading,
    config::APP_CONFIG,
    services::{
        subscription_service::{BillingInterval, SubscriptionCommand},
        user_service::CONNECTED_USER,
    },
    utils::current_location,
};

#[component]
pub fn SubscriptionCard() -> Element {
    let subscription_service = use_coroutine_handle::<SubscriptionCommand>();

    let Some(user_context) = CONNECTED_USER.read().clone() else {
        return rsx! {
            div {
                class: "card w-full bg-base-200",
                Loading { label: "Loading subscription..." }
            }
        };
    };

    let subscription = &user_context.subscription;
    let is_stripe_enabled = APP_CONFIG
        .read()
        .as_ref()
        .map(|config| config.stripe_enabled)
        .unwrap_or(false);

    let (status_badge, status_icon, status_text) = match subscription.status {
        SubscriptionStatus::Trialing => (
            "badge-info badge-soft",
            rsx! { Icon { class: "min-w-5 h-5", icon: BsHourglass } },
            "Trial",
        ),
        SubscriptionStatus::Active => (
            "badge-success badge-soft",
            rsx! { Icon { class: "min-w-5 h-5", icon: BsCheck } },
            "Active",
        ),
        SubscriptionStatus::PastDue => (
            "badge-warning badge-soft",
            rsx! { Icon { class: "min-w-5 h-5", icon: BsExclamationTriangle } },
            "Past Due",
        ),
        SubscriptionStatus::Canceled => (
            "badge-error badge-soft",
            rsx! { Icon { class: "min-w-5 h-5", icon: BsXCircle } },
            "Canceled",
        ),
        SubscriptionStatus::Expired => (
            "badge-error badge-soft",
            rsx! { Icon { class: "min-w-5 h-5", icon: BsXCircle } },
            "Expired",
        ),
        SubscriptionStatus::Unlimited => (
            "badge-success badge-soft",
            rsx! { Icon { class: "min-w-5 h-5", icon: BsInfinity } },
            "Unlimited",
        ),
    };

    let show_subscribe_button = matches!(
        subscription.status,
        SubscriptionStatus::Trialing | SubscriptionStatus::Expired | SubscriptionStatus::Canceled
    );
    let show_manage_button = matches!(
        subscription.status,
        SubscriptionStatus::Active | SubscriptionStatus::PastDue
    );

    let current_url = current_location()
        .map(|url| url.to_string())
        .unwrap_or_default();

    rsx! {
        div {
            class: "card w-full bg-base-200",

            div {
                class: "card-body",

                div {
                    class: "flex flex-col sm:flex-row justify-between items-start sm:items-center gap-4",

                    div {
                        class: "flex gap-3 items-center",
                        Icon { class: "w-8 h-8 text-primary", icon: BsCreditCard }
                        h2 { class: "card-title", "Subscription" }
                    }

                    span {
                        class: "badge {status_badge} gap-1",
                        {status_icon}
                        span { "{status_text}" }
                    }
                }

                div {
                    class: "mt-4 flex flex-col gap-2",

                    if let Some(days) = subscription.days_remaining {
                        div {
                            class: "flex gap-2 text-sm",
                            span { class: "text-base-content/70", "Days remaining:" }
                            span { class: "font-semibold", "{days}" }
                        }
                    }

                    if let Some(interval) = subscription.billing_interval {
                        div {
                            class: "flex gap-2 text-sm",
                            span { class: "text-base-content/70", "Billing:" }
                            span { class: "font-semibold",
                                match interval {
                                    universal_inbox::subscription::BillingInterval::Month => "Monthly",
                                    universal_inbox::subscription::BillingInterval::Year => "Annually",
                                }
                            }
                        }
                    }

                    if subscription.is_read_only {
                        div {
                            class: "alert alert-warning alert-soft rounded-md mt-2 text-sm flex gap-2",
                            role: "alert",
                            Icon { class: "min-w-5 h-5", icon: BsExclamationTriangle }
                            span { "Your account is in read-only mode. Subscribe to regain full access." }
                        }
                    }
                }

                if is_stripe_enabled {
                    div {
                        class: "card-actions mt-4 flex justify-end gap-2",

                        if show_subscribe_button {
                            button {
                                class: "btn btn-primary btn-sm",
                                onclick: {
                                    let current_url = current_url.clone();
                                    move |_| {
                                        subscription_service.send(SubscriptionCommand::CreateCheckoutSession {
                                            billing_interval: BillingInterval::Monthly,
                                            success_url: format!("{}?subscription=success", current_url),
                                            cancel_url: format!("{}?subscription=canceled", current_url),
                                        });
                                    }
                                },
                                "Subscribe Monthly"
                            }
                            button {
                                class: "btn btn-primary btn-soft btn-sm",
                                onclick: {
                                    let current_url = current_url.clone();
                                    move |_| {
                                        subscription_service.send(SubscriptionCommand::CreateCheckoutSession {
                                            billing_interval: BillingInterval::Annual,
                                            success_url: format!("{}?subscription=success", current_url),
                                            cancel_url: format!("{}?subscription=canceled", current_url),
                                        });
                                    }
                                },
                                "Subscribe Annually"
                            }
                        }

                        if show_manage_button {
                            button {
                                class: "btn btn-primary btn-sm",
                                onclick: {
                                    let current_url = current_url.clone();
                                    move |_| {
                                        subscription_service.send(SubscriptionCommand::OpenBillingPortal {
                                            return_url: current_url.clone(),
                                        });
                                    }
                                },
                                "Manage Subscription"
                            }
                        }
                    }
                }
            }
        }
    }
}
