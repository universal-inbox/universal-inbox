use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};
use uuid::Uuid;

use crate::user::UserId;

#[derive(Debug, Serialize, Deserialize, PartialEq, Copy, Clone, Eq, Hash)]
#[serde(transparent)]
pub struct SubscriptionId(pub Uuid);

impl fmt::Display for SubscriptionId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for SubscriptionId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl From<SubscriptionId> for Uuid {
    fn from(id: SubscriptionId) -> Self {
        id.0
    }
}

impl TryFrom<String> for SubscriptionId {
    type Error = uuid::Error;

    fn try_from(uuid: String) -> Result<Self, Self::Error> {
        Ok(Self(Uuid::parse_str(&uuid)?))
    }
}

impl FromStr for SubscriptionId {
    type Err = uuid::Error;

    fn from_str(uuid: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(uuid)?))
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Copy, Hash, Display, EnumString)]
pub enum SubscriptionStatus {
    Trialing,
    Active,
    PastDue,
    Canceled,
    Expired,
    Unlimited,
}

impl Default for SubscriptionStatus {
    fn default() -> Self {
        Self::Trialing
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Copy, Hash, Display, EnumString)]
pub enum BillingInterval {
    #[strum(serialize = "month")]
    Month,
    #[strum(serialize = "year")]
    Year,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct UserSubscription {
    pub id: SubscriptionId,
    pub user_id: UserId,
    pub stripe_customer_id: Option<String>,
    pub subscription_status: SubscriptionStatus,
    pub subscription_id: Option<String>,
    pub trial_started_at: Option<DateTime<Utc>>,
    pub trial_ends_at: Option<DateTime<Utc>>,
    pub subscription_ends_at: Option<DateTime<Utc>>,
    pub billing_interval: Option<BillingInterval>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Summary of user subscription status for API responses
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubscriptionInfo {
    pub status: SubscriptionStatus,
    pub trial_ends_at: Option<DateTime<Utc>>,
    pub subscription_ends_at: Option<DateTime<Utc>>,
    pub billing_interval: Option<BillingInterval>,
    pub days_remaining: Option<i64>,
    pub is_read_only: bool,
}

impl SubscriptionInfo {
    pub fn from_subscription(subscription: &UserSubscription) -> Self {
        let days_remaining = Self::compute_days_remaining(subscription);
        Self {
            status: subscription.subscription_status,
            trial_ends_at: subscription.trial_ends_at,
            subscription_ends_at: subscription.subscription_ends_at,
            billing_interval: subscription.billing_interval,
            days_remaining,
            is_read_only: subscription.is_read_only(),
        }
    }

    pub fn unlimited() -> Self {
        Self {
            status: SubscriptionStatus::Unlimited,
            trial_ends_at: None,
            subscription_ends_at: None,
            billing_interval: None,
            days_remaining: None,
            is_read_only: false,
        }
    }

    fn compute_days_remaining(subscription: &UserSubscription) -> Option<i64> {
        let now = Utc::now();
        match subscription.subscription_status {
            SubscriptionStatus::Trialing => subscription
                .trial_ends_at
                .map(|end| (end - now).num_days().max(0)),
            SubscriptionStatus::Active | SubscriptionStatus::Canceled => subscription
                .subscription_ends_at
                .map(|end| (end - now).num_days().max(0)),
            SubscriptionStatus::PastDue
            | SubscriptionStatus::Expired
            | SubscriptionStatus::Unlimited => None,
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self.status,
            SubscriptionStatus::Active
                | SubscriptionStatus::Trialing
                | SubscriptionStatus::Unlimited
        )
    }

    pub fn is_read_only(&self) -> bool {
        matches!(
            self.status,
            SubscriptionStatus::Expired
                | SubscriptionStatus::Canceled
                | SubscriptionStatus::PastDue
        )
    }
}

impl UserSubscription {
    pub fn new_trial(user_id: UserId, trial_ends_at: DateTime<Utc>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().into(),
            user_id,
            stripe_customer_id: None,
            subscription_status: SubscriptionStatus::Trialing,
            subscription_id: None,
            trial_started_at: Some(now),
            trial_ends_at: Some(trial_ends_at),
            subscription_ends_at: None,
            billing_interval: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn unlimited(user_id: UserId) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().into(),
            user_id,
            stripe_customer_id: None,
            subscription_status: SubscriptionStatus::Unlimited,
            subscription_id: None,
            trial_started_at: None,
            trial_ends_at: None,
            subscription_ends_at: None,
            billing_interval: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self.subscription_status,
            SubscriptionStatus::Active
                | SubscriptionStatus::Trialing
                | SubscriptionStatus::Unlimited
        )
    }

    pub fn is_read_only(&self) -> bool {
        matches!(
            self.subscription_status,
            SubscriptionStatus::Expired
                | SubscriptionStatus::Canceled
                | SubscriptionStatus::PastDue
        )
    }
}
