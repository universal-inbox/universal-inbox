use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum::EnumString;

use crate::user::UserId;

/// Stable error code on the 402 returned when a Free user tries to add too
/// many integrations. Lives in the shared crate so the API, the web upgrade
/// modal, and the API tests all reference one constant instead of repeating
/// the bare string, where a rename would break the modal silently.
pub const FREE_PLAN_INTEGRATION_LIMIT_CODE: &str = "free_plan_integration_limit_reached";

/// A Stripe Customer id (`cus_…`). Newtype so it can't be mixed up with a
/// price/subscription id at a call site. `serde(transparent)` keeps the wire
/// form a bare string. A user with no Stripe customer yet is represented by
/// `Option::None`, not a sentinel value.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Hash)]
#[serde(transparent)]
pub struct StripeCustomerId(pub String);

impl StripeCustomerId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StripeCustomerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A Stripe Subscription id (`sub_…`). Newtype for the same reason as
/// [`StripeCustomerId`].
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Hash)]
#[serde(transparent)]
pub struct StripeSubscriptionId(pub String);

impl StripeSubscriptionId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StripeSubscriptionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A Stripe Price id (`price_…`). Newtype for the same reason as
/// [`StripeCustomerId`].
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Hash)]
#[serde(transparent)]
pub struct StripePriceId(pub String);

impl StripePriceId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StripePriceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Plan a user belongs to. `Free` is the default for any user lacking a
/// persisted subscription row; `Paid` requires an active Stripe subscription.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Copy, Hash, Default, EnumString)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum Plan {
    #[default]
    Free,
    Paid,
}

impl Plan {
    pub fn is_paid(&self) -> bool {
        matches!(self, Plan::Paid)
    }
}

impl fmt::Display for Plan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Plan::Free => f.write_str("free"),
            Plan::Paid => f.write_str("paid"),
        }
    }
}

/// Mirrors Stripe's subscription lifecycle statuses. We map directly from the
/// `customer.subscription.*` webhook payloads so downstream code can switch
/// on a single concrete enum.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Copy, Hash, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum SubscriptionStatus {
    Active,
    PastDue,
    Canceled,
    Unpaid,
    Incomplete,
    IncompleteExpired,
    Trialing,
    Paused,
}

impl SubscriptionStatus {
    /// Whether this status entitles the user to Paid-plan behaviour.
    ///
    /// During Stripe's smart-retry dunning window the status is `past_due`;
    /// per goals/stripe-billing/facts.md the user keeps Paid entitlements
    /// until Stripe definitively cancels the subscription. `trialing` is
    /// included for forward compatibility even though no trial is in scope
    /// at launch.
    pub fn is_paid_entitled(&self) -> bool {
        matches!(
            self,
            SubscriptionStatus::Active | SubscriptionStatus::PastDue | SubscriptionStatus::Trialing
        )
    }

    /// Whether the subscription is in a terminal state (no further billing
    /// activity expected without manual intervention).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            SubscriptionStatus::Canceled
                | SubscriptionStatus::Unpaid
                | SubscriptionStatus::IncompleteExpired
        )
    }
}

impl fmt::Display for SubscriptionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            SubscriptionStatus::Active => "active",
            SubscriptionStatus::PastDue => "past_due",
            SubscriptionStatus::Canceled => "canceled",
            SubscriptionStatus::Unpaid => "unpaid",
            SubscriptionStatus::Incomplete => "incomplete",
            SubscriptionStatus::IncompleteExpired => "incomplete_expired",
            SubscriptionStatus::Trialing => "trialing",
            SubscriptionStatus::Paused => "paused",
        };
        f.write_str(s)
    }
}

/// Per-user subscription state, persisted in the API's `user_subscription`
/// table. Absence of a row for a given `user_id` is equivalent to a row with
/// `plan = Free, status = Canceled`.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct UserSubscription {
    pub user_id: UserId,
    pub stripe_customer_id: Option<StripeCustomerId>,
    pub stripe_subscription_id: Option<StripeSubscriptionId>,
    pub stripe_price_id: Option<StripePriceId>,
    pub plan: Plan,
    pub status: SubscriptionStatus,
    pub current_period_start: Option<DateTime<Utc>>,
    pub current_period_end: Option<DateTime<Utc>>,
    pub cancel_at_period_end: bool,
    pub canceled_at: Option<DateTime<Utc>>,
    /// Deadline beyond which over-limit Free users have their excess
    /// integration connections auto-paused. Set once when the user is first
    /// observed as over-limit and unset when they return under the limit.
    pub over_limit_grace_deadline: Option<DateTime<Utc>>,
    /// Stripe `event.created` of the most recent subscription-state webhook
    /// applied to this row. Used as an ordering guard: Stripe does not
    /// guarantee delivery order, so the webhook handler ignores any event
    /// whose `created` is older than this value (e.g. a retried stale
    /// `subscription.updated` arriving after `subscription.deleted`). `None`
    /// until the first subscription event lands.
    pub last_stripe_event_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl UserSubscription {
    /// The row as it stands, for a write that means to change only a field or
    /// two of it — linking a Stripe customer, say. `None` yields a fresh Free
    /// row. Spread it and override what the write is actually about, so the
    /// other fields cannot be reset by accident:
    /// `UserSubscription { stripe_customer_id: Some(id), ..carry_forward(..) }`.
    pub fn carry_forward(
        user_id: UserId,
        existing: Option<&UserSubscription>,
        now: DateTime<Utc>,
    ) -> UserSubscription {
        match existing {
            Some(existing) => UserSubscription {
                updated_at: now,
                ..existing.clone()
            },
            None => UserSubscription {
                user_id,
                stripe_customer_id: None,
                stripe_subscription_id: None,
                stripe_price_id: None,
                plan: Plan::Free,
                // `Incomplete`, not `Canceled`: the reconcile query excludes
                // canceled rows, so a `Canceled` placeholder's grace deadline
                // would never be picked up. Both resolve to Free.
                status: SubscriptionStatus::Incomplete,
                current_period_start: None,
                current_period_end: None,
                cancel_at_period_end: false,
                canceled_at: None,
                over_limit_grace_deadline: None,
                last_stripe_event_at: None,
                created_at: now,
                updated_at: now,
            },
        }
    }

    /// Resolves the effective plan for entitlement decisions. The persisted
    /// `plan` is taken at face value only when the `status` still entitles
    /// the user to Paid behaviour.
    pub fn effective_plan(&self) -> Plan {
        if self.plan.is_paid() && self.status.is_paid_entitled() {
            Plan::Paid
        } else {
            Plan::Free
        }
    }
}

/// Operator-configured free-plan limits, mirrored from `[billing.free_plan]`.
/// Lives in the shared crate so the frontend can read it (e.g. for the
/// upgrade modal copy) without recompiling against the API.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct BillingLimits {
    pub max_integration_connections: u32,
    pub notification_sync_interval_in_minutes: i64,
    pub task_sync_interval_in_minutes: i64,
    pub rollout_grace_days: u32,
}

/// Plan + usage snapshot exchanged between the API's `/billing/me` route and
/// the web frontend. Defined once in the shared crate (both sides depend on
/// it) so a field rename is a compile error on both ends rather than a
/// runtime deserialization failure on the web only.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BillingStateResponse {
    pub plan: Plan,
    pub status: Option<SubscriptionStatus>,
    pub current_period_end: Option<DateTime<Utc>>,
    pub cancel_at_period_end: bool,
    pub integration_usage: IntegrationUsage,
    pub sync_interval: SyncIntervals,
    pub upgrade_available: bool,
    pub manage_billing_available: bool,
    pub over_limit_grace_deadline: Option<DateTime<Utc>>,
}

impl BillingStateResponse {
    pub fn is_free(&self) -> bool {
        self.plan == Plan::Free
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntegrationUsage {
    /// Connections counting against the cap: validated and not paused by the
    /// plan.
    pub used: u32,
    pub limit: Option<u32>,
    /// Connections the plan paused. They consume no slot and come back on
    /// upgrade, so they are reported beside `used` rather than inside it.
    pub paused_by_plan: u32,
}

/// Effective minimum sync intervals (in minutes) actually enforced for this
/// user — `max(global floor, plan floor)`, not the raw plan floor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncIntervals {
    pub notification_minutes: i64,
    pub task_minutes: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_default_is_free() {
        assert_eq!(Plan::default(), Plan::Free);
    }

    #[test]
    fn paid_active_grants_paid_entitlement() {
        assert!(SubscriptionStatus::Active.is_paid_entitled());
        assert!(SubscriptionStatus::PastDue.is_paid_entitled());
        assert!(SubscriptionStatus::Trialing.is_paid_entitled());
    }

    #[test]
    fn terminal_statuses_do_not_grant_paid_entitlement() {
        for status in [
            SubscriptionStatus::Canceled,
            SubscriptionStatus::Unpaid,
            SubscriptionStatus::Incomplete,
            SubscriptionStatus::IncompleteExpired,
            SubscriptionStatus::Paused,
        ] {
            assert!(!status.is_paid_entitled(), "{status:?} should not entitle");
        }
    }

    #[test]
    fn effective_plan_drops_to_free_when_status_is_terminal() {
        let sub = UserSubscription {
            user_id: UserId(uuid::Uuid::new_v4()),
            stripe_customer_id: Some(StripeCustomerId("cus_test".to_string())),
            stripe_subscription_id: Some(StripeSubscriptionId("sub_test".to_string())),
            stripe_price_id: Some(StripePriceId("price_test".to_string())),
            plan: Plan::Paid,
            status: SubscriptionStatus::Canceled,
            current_period_start: None,
            current_period_end: None,
            cancel_at_period_end: false,
            canceled_at: Some(Utc::now()),
            over_limit_grace_deadline: None,
            last_stripe_event_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        assert_eq!(sub.effective_plan(), Plan::Free);
    }

    #[test]
    fn effective_plan_paid_when_active() {
        let sub = UserSubscription {
            user_id: UserId(uuid::Uuid::new_v4()),
            stripe_customer_id: Some(StripeCustomerId("cus_test".to_string())),
            stripe_subscription_id: Some(StripeSubscriptionId("sub_test".to_string())),
            stripe_price_id: Some(StripePriceId("price_test".to_string())),
            plan: Plan::Paid,
            status: SubscriptionStatus::Active,
            current_period_start: None,
            current_period_end: None,
            cancel_at_period_end: false,
            canceled_at: None,
            over_limit_grace_deadline: None,
            last_stripe_event_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        assert_eq!(sub.effective_plan(), Plan::Paid);
    }

    #[test]
    fn plan_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&Plan::Free).unwrap(), "\"free\"");
        assert_eq!(serde_json::to_string(&Plan::Paid).unwrap(), "\"paid\"");
    }

    #[test]
    fn subscription_status_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&SubscriptionStatus::PastDue).unwrap(),
            "\"past_due\""
        );
        assert_eq!(
            serde_json::to_string(&SubscriptionStatus::IncompleteExpired).unwrap(),
            "\"incomplete_expired\""
        );
    }
}
