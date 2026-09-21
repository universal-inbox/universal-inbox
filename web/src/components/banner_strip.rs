#![allow(non_snake_case)]
//! Stack of dismissible billing banners. Surfaces three states from
//! `BILLING_STATE`:
//!
//! 1. `past_due` — Stripe smart retries are running; the user must update
//!    their card before the subscription cancels.
//! 2. `cancel_at_period_end` — user cancelled but still has Paid access
//!    until `current_period_end`.
//! 3. `over_limit_grace_deadline` — Free user is over the integration cap
//!    and has N days to upgrade / disconnect before auto-pause kicks in.
//!
//! Renders nothing when none of the conditions apply, and nothing on
//! self-hosted instances (BillingAvailability::Disabled).

use dioxus::prelude::*;
use universal_inbox::billing::SubscriptionStatus;

use crate::services::billing_service::{
    BILLING_STATE, BillingAvailability, BillingCommand, open_checkout, open_portal,
};

#[component]
pub fn BillingBannerStrip() -> Element {
    let state = BILLING_STATE.read().clone();
    let billing = match state {
        BillingAvailability::Enabled(s) => s,
        _ => return rsx! { Fragment {} },
    };

    let mut banners: Vec<BannerProps> = vec![];

    if billing.status == Some(SubscriptionStatus::PastDue) {
        banners.push(BannerProps {
            tone: BannerTone::Error,
            message: "Your last payment failed. Update your card to keep your Paid plan."
                .to_string(),
            action_label: Some("Manage billing".to_string()),
            action: Some(BannerAction::OpenPortal),
        });
    }

    if billing.cancel_at_period_end && billing.plan.is_paid() {
        let when = billing
            .current_period_end
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "soon".to_string());
        banners.push(BannerProps {
            tone: BannerTone::Warning,
            message: format!(
                "Your subscription ends on {when}. Reactivate any time from Settings → Billing."
            ),
            action_label: Some("Manage billing".to_string()),
            action: Some(BannerAction::OpenPortal),
        });
    }

    // Paused integrations are the *settled* over-limit state: the deadline has
    // already fired and the excess is off. Say so, instead of counting down to
    // something that has happened.
    let paused = billing.integration_usage.paused_by_plan;
    if paused > 0 && billing.is_free() {
        let plural = if paused == 1 { "" } else { "s" };
        banners.push(BannerProps {
            tone: BannerTone::Warning,
            message: format!(
                "{paused} integration{plural} paused by your Free plan. Upgrade to bring them back."
            ),
            action_label: Some("Upgrade".to_string()),
            action: Some(BannerAction::OpenCheckout),
        });
    } else if let Some(deadline) = billing.over_limit_grace_deadline
        && billing.is_free()
    {
        let max = billing.integration_usage.limit.unwrap_or(0);
        let used = billing.integration_usage.used;
        let when = deadline.format("%Y-%m-%d");
        banners.push(BannerProps {
            tone: BannerTone::Warning,
            message: format!(
                "You currently use {used} integrations. After {when} your free plan allows {max} — \
                 upgrade or choose which to keep."
            ),
            action_label: Some("Upgrade".to_string()),
            action: Some(BannerAction::OpenCheckout),
        });
    }

    if banners.is_empty() {
        return rsx! { Fragment {} };
    }

    rsx! {
        div {
            class: "flex flex-col gap-0",
            for banner in banners {
                Banner { ..banner }
            }
        }
    }
}

#[derive(Clone, PartialEq, Props)]
struct BannerProps {
    tone: BannerTone,
    message: String,
    action_label: Option<String>,
    action: Option<BannerAction>,
}

#[derive(Clone, PartialEq)]
enum BannerTone {
    Error,
    Warning,
}

#[derive(Clone, PartialEq)]
enum BannerAction {
    OpenCheckout,
    OpenPortal,
}

#[component]
fn Banner(props: BannerProps) -> Element {
    let class = match props.tone {
        BannerTone::Error => {
            "w-full bg-ui-error text-ui-on-brand px-4 py-2 text-sm flex items-center justify-center gap-3"
        }
        BannerTone::Warning => {
            "w-full bg-ui-warning text-ui-on-brand px-4 py-2 text-sm flex items-center justify-center gap-3"
        }
    };
    let billing_service = use_coroutine_handle::<BillingCommand>();

    rsx! {
        div { class: "{class}",
            span { "{props.message}" }
            if let (Some(label), Some(action)) = (props.action_label.clone(), props.action.clone()) {
                button {
                    class: "underline font-medium bg-transparent border-0 cursor-pointer",
                    onclick: move |_| {
                        match action {
                            BannerAction::OpenCheckout => billing_service.send(open_checkout()),
                            BannerAction::OpenPortal => billing_service.send(open_portal()),
                        }
                    },
                    "{label}"
                }
            }
        }
    }
}
