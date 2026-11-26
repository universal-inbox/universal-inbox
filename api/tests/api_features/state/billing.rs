use chrono::{DateTime, Utc};

use universal_inbox::{integration_connection::IntegrationConnectionId, user::UserId};
use universal_inbox_api::billing::{service::ReconcileReport, stripe::RawSubscription};

#[derive(Default)]
pub struct Billing {
    pub user_id: Option<UserId>,
    pub stripe_holds: Vec<RawSubscription>,
    pub seeded: Vec<IntegrationConnectionId>,
    pub calendar: Option<IntegrationConnectionId>,
    pub armed: Vec<Option<DateTime<Utc>>>,
    pub paused: Vec<Vec<IntegrationConnectionId>>,
    pub snapshot:
        Option<universal_inbox::integration_connection::config::IntegrationConnectionConfig>,
    pub report: Option<ReconcileReport>,
    pub written_status: Option<universal_inbox::billing::SubscriptionStatus>,
    pub used: Option<u32>,
    pub verdict: Option<Result<(), String>>,
    pub stripe_mock: Option<wiremock::MockServer>,
    pub deletion: Option<Result<bool, String>>,
    pub other_user_id: Option<UserId>,
    pub others_subscription: Option<universal_inbox::billing::UserSubscription>,
    pub interval: Option<i64>,
    pub pruned: Option<u64>,
}
