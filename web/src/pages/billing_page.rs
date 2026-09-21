#![allow(non_snake_case)]
//! Settings → Billing page.
//!
//! Renders only when `BILLING_STATE` reports `Enabled` — i.e. the API has
//! `[billing]` configured. On self-hosted instances the route still exists
//! but shows a "Billing is not enabled" message rather than 404'ing, so the
//! UX stays consistent if the user types `/billing` directly.
//!
//! The layout mirrors the Security page: a `PageHeader` followed by a stack
//! of design-system `Card`s (a plan hero card carrying the actions, then a
//! usage card with metric tiles). It reuses the same tokens, animation, and
//! `max-w-3xl` container so Billing reads as part of the same product chrome.

use dioxus::prelude::*;
use log::debug;

use crate::{
    components::{
        loading::Loading,
        ui::{
            Button, ButtonVariant, Card, CardHeader, CardMeta, CardRight, CardVariant, PageHeader,
            StatusLeaf, StatusLeafVariant,
        },
    },
    services::billing_service::{
        BILLING_STATE, BillingAvailability, BillingCommand, BillingState, open_checkout,
        open_portal,
    },
};

pub fn BillingPage() -> Element {
    let billing_service = use_coroutine_handle::<BillingCommand>();
    debug!("Rendering billing page");

    // Trigger a refresh whenever the page renders so the user sees a current
    // snapshot even after coming back from a Stripe Checkout / Portal redirect.
    let _resource = use_resource(move || {
        to_owned![billing_service];
        async move {
            billing_service.send(BillingCommand::Refresh);
        }
    });

    let state = BILLING_STATE.read().clone();

    rsx! {
        div {
            class: "flex-1 overflow-y-auto bg-ui-base-200",

            div {
                class: "px-5 pt-4 pb-10 max-w-3xl mx-auto flex flex-col gap-4 animate-detail-fade",

                PageHeader {
                    title: "Billing".to_string(),
                    subtitle: Some(
                        "Review your plan, usage, and payment details.".to_string(),
                    ),
                }

                match state {
                    BillingAvailability::Unknown => rsx! {
                        Card { variant: CardVariant::ApiKeys,
                            Loading { label: "Loading billing details..." }
                        }
                    },
                    BillingAvailability::Disabled => rsx! {
                        BillingDisabledNotice {}
                    },
                    BillingAvailability::Enabled(state) => rsx! {
                        BillingDetails { state }
                    },
                }
            }
        }
    }
}

#[component]
fn BillingDisabledNotice() -> Element {
    rsx! {
        Card { variant: CardVariant::ApiKeys,
            div {
                class: "flex items-start gap-3.5 p-3.5",
                span { class: "icon-[lucide--credit-card] size-[18px] text-ui-base-muted mt-px" }
                div { class: "flex-1 min-w-0",
                    div { class: "text-[13.5px] font-bold tracking-[-0.01em]", "Billing is not enabled" }
                    p {
                        class: "text-[12.5px] text-ui-base-muted leading-normal mt-1",
                        "Universal Inbox is running without Stripe — no plan, no "
                        "limits, no payment is required on this instance."
                    }
                }
            }
        }
    }
}

#[component]
fn BillingDetails(state: BillingState) -> Element {
    let billing_service = use_coroutine_handle::<BillingCommand>();

    let is_paid = state.plan.is_paid();
    // A paid subscription scheduled to cancel (at period end or a specific
    // `cancel_at`) is still entitled until the period ends, but the user should
    // clearly see it's winding down — surface it as its own "Canceling" state.
    let is_canceling = is_paid && state.cancel_at_period_end;
    let plan_label = if is_paid { "Paid" } else { "Free" };
    let (leaf_variant, leaf_label) = if is_canceling {
        (StatusLeafVariant::SyncIssue, "Canceling".to_string())
    } else if is_paid {
        (StatusLeafVariant::Connected, "Active".to_string())
    } else {
        (
            StatusLeafVariant::Disconnected,
            "No subscription".to_string(),
        )
    };

    rsx! {
        // ── Plan hero card — plan name + status, with the billing actions
        //    on the right (mirrors the Security card-header action layout).
        Card { variant: CardVariant::ApiKeys,
            div {
                class: "flex items-start gap-3.5 p-3.5 max-md:flex-wrap",

                span { class: "icon-[lucide--credit-card] size-[22px] text-ui-primary mt-0.5 shrink-0" }

                div { class: "flex-1 min-w-0",
                    div { class: "flex items-center gap-2",
                        span {
                            class: "text-[18px] font-bold tracking-[-0.01em] text-ui-base-content",
                            "{plan_label} plan"
                        }
                        StatusLeaf { variant: leaf_variant, label: leaf_label }
                    }

                    if is_canceling {
                        // Prominent cancellation notice — same attention styling
                        // as the over-limit warning so a winding-down subscription
                        // can't be mistaken for an active, renewing one.
                        div {
                            class: "flex items-start gap-2 mt-2 rounded-ui-sm border border-ui-warning \
                                    bg-ui-warning-subtle px-3 py-2.5 text-[12px] text-ui-warning-text leading-snug",
                            span { class: "icon-[lucide--alert-triangle] size-[15px] shrink-0 mt-px text-ui-warning" }
                            div {
                                "Your subscription is canceled. You keep Paid access until "
                                if let Some(period_end) = state.current_period_end {
                                    span {
                                        class: "font-semibold",
                                        "{period_end.format(\"%Y-%m-%d\")}"
                                    }
                                    ", then you'll move to the Free plan."
                                } else {
                                    "the end of the current period, then you'll move to the Free plan."
                                }
                            }
                        }
                    } else if is_paid {
                        // Active, non-canceling paid subscription — it renews.
                        p {
                            class: "text-[12.5px] text-ui-base-muted leading-snug mt-1.5",
                            if let Some(period_end) = state.current_period_end {
                                "Next renewal on "
                                span {
                                    class: "text-ui-base-content font-medium",
                                    "{period_end.format(\"%Y-%m-%d\")}"
                                }
                                "."
                            } else {
                                "Your subscription renews automatically."
                            }
                        }
                    } else {
                        // Free plan — a canceled subscription keeps a (final)
                        // `current_period_end` from Stripe, so don't gate on it
                        // here or a former subscriber sees a misleading
                        // "Next renewal on …".
                        p {
                            class: "text-[12.5px] text-ui-base-muted leading-snug mt-1.5",
                            if state.upgrade_available {
                                "No recurring charge on the Free plan. Upgrade to unlock "
                                "unlimited integrations and faster sync."
                            } else {
                                "No recurring charge."
                            }
                        }
                    }
                }

                CardRight {
                    class: "max-md:basis-full max-md:justify-end".to_string(),
                    if state.manage_billing_available {
                        Button {
                            variant: ButtonVariant::Ghost,
                            icon_class: "icon-[lucide--settings]".to_string(),
                            onclick: move |_| {
                                billing_service.send(open_portal());
                            },
                            "Manage billing"
                        }
                    }
                    if state.upgrade_available {
                        Button {
                            variant: ButtonVariant::Primary,
                            icon_class: "icon-[lucide--arrow-up-circle]".to_string(),
                            onclick: move |_| {
                                billing_service.send(open_checkout());
                            },
                            "Upgrade to Paid"
                        }
                    }
                }
            }
        }

        // ── Usage card — metrics measured against the current plan limits.
        Card { variant: CardVariant::ApiKeys,
            CardHeader {
                interactive: false,
                span { class: "icon-[lucide--activity] size-[18px]" }
                CardMeta {
                    name: "Usage".to_string(),
                    description: rsx! { "Measured against your current plan limits" },
                }
            }

            div { class: "border-t border-ui-border-light" }

            div { class: "p-3.5",
                div {
                    class: "grid grid-cols-2 gap-3.5 max-md:grid-cols-1",
                    MetricCard {
                        icon_class: "icon-[lucide--plug]".to_string(),
                        label: "Integrations connected".to_string(),
                        value: match state.integration_usage.limit {
                            Some(limit) => format!("{}/{}", state.integration_usage.used, limit),
                            None => format!("{}", state.integration_usage.used),
                        },
                        progress: state.integration_usage.limit.map(|limit| {
                            if limit == 0 {
                                1.0
                            } else {
                                (state.integration_usage.used as f64 / limit as f64).clamp(0.0, 1.0)
                            }
                        }),
                        progress_warning: state
                            .integration_usage
                            .limit
                            .is_some_and(|limit| state.integration_usage.used >= limit),
                    }
                    MetricCard {
                        icon_class: "icon-[lucide--refresh-cw]".to_string(),
                        label: "Sync interval".to_string(),
                        value: format_sync_interval(state.sync_interval.notification_minutes),
                    }
                }

                if state.integration_usage.paused_by_plan > 0 {
                    div {
                        class: "flex items-start gap-2 mt-3.5 rounded-ui-sm border border-ui-warning \
                                bg-ui-warning-subtle px-3 py-2.5 text-[12px] text-ui-warning-text leading-snug",
                        span { class: "icon-[lucide--pause-circle] size-[15px] shrink-0 mt-px text-ui-warning" }
                        div {
                            span {
                                class: "font-semibold",
                                "{state.integration_usage.paused_by_plan}"
                            }
                            " integrations are paused because your Free plan allows "
                            "{state.integration_usage.limit.unwrap_or(0)}. They stop syncing until you "
                            "upgrade, which restores every one of them."
                        }
                    }
                } else if let Some(deadline) = state.over_limit_grace_deadline {
                    div {
                        class: "flex items-start gap-2 mt-3.5 rounded-ui-sm border border-ui-warning \
                                bg-ui-warning-subtle px-3 py-2.5 text-[12px] text-ui-warning-text leading-snug",
                        span { class: "icon-[lucide--alert-triangle] size-[15px] shrink-0 mt-px text-ui-warning" }
                        div {
                            "You use more integrations than your Free plan allows. After "
                            span {
                                class: "font-semibold",
                                "{deadline.format(\"%Y-%m-%d\")}"
                            }
                            " excess integrations will be paused automatically. Upgrade or "
                            "disconnect to choose which to keep."
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn MetricCard(
    icon_class: String,
    label: String,
    value: String,
    /// Optional usage fraction (0.0–1.0) rendered as a progress bar below the
    /// value. `None` hides the bar (e.g. for the sync-interval metric).
    #[props(default)]
    progress: Option<f64>,
    /// Tint the progress bar with the warning color when the metric is at or
    /// over its limit.
    #[props(default)]
    progress_warning: bool,
) -> Element {
    let bar_color = if progress_warning {
        "bg-ui-warning"
    } else {
        "bg-ui-primary"
    };

    rsx! {
        div {
            class: "rounded-ui-sm border border-ui-border p-4",
            div {
                class: "flex items-center gap-1.5 text-xs uppercase tracking-wide text-ui-base-muted mb-1",
                span { class: "{icon_class} size-3.5" }
                "{label}"
            }
            div {
                class: "text-[18px] font-bold tracking-[-0.01em] text-ui-base-content",
                "{value}"
            }
            if let Some(progress) = progress {
                {
                    let pct = (progress * 100.0).round();
                    rsx! {
                        div {
                            class: "h-1.5 rounded-ui-pill bg-ui-surface-alt overflow-hidden mt-2",
                            div {
                                class: "h-full rounded-ui-pill {bar_color}",
                                style: "width: {pct}%",
                            }
                        }
                    }
                }
            }
        }
    }
}

fn format_sync_interval(minutes: i64) -> String {
    if minutes <= 0 {
        "Live".to_string()
    } else if minutes % 1440 == 0 && minutes >= 1440 {
        let days = minutes / 1440;
        if days == 1 {
            "Once a day".to_string()
        } else {
            format!("Every {days} days")
        }
    } else if minutes % 60 == 0 {
        let hours = minutes / 60;
        if hours == 1 {
            "Every hour".to_string()
        } else {
            format!("Every {hours} hours")
        }
    } else {
        format!("Every {minutes} minutes")
    }
}
