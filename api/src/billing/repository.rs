//! Persistence layer for the optional Stripe billing subsystem.
//!
//! Trait + impl on the existing [`Repository`](crate::repository::Repository)
//! so callers don't need a second pool/handle; the impl is kept inside
//! `api/src/billing/` so deleting the billing tree leaves the rest of the
//! repository module untouched.

use anyhow::anyhow;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Postgres, Row, Transaction};
use tracing::warn;

use universal_inbox::{
    billing::{
        Plan, StripeCustomerId, StripePriceId, StripeSubscriptionId, SubscriptionStatus,
        UserSubscription,
    },
    user::UserId,
};

use crate::observability::attr;
use crate::{repository::Repository, universal_inbox::UniversalInboxError};

#[async_trait]
pub trait BillingRepository {
    /// Fetch a user's persisted subscription row, if any. Absence is equivalent
    /// to "Free plan, no Stripe linkage."
    async fn get_user_subscription(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Option<UserSubscription>, UniversalInboxError>;

    async fn get_user_subscription_by_customer_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        stripe_customer_id: &str,
    ) -> Result<Option<UserSubscription>, UniversalInboxError>;

    async fn get_user_subscription_by_subscription_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        stripe_subscription_id: &str,
    ) -> Result<Option<UserSubscription>, UniversalInboxError>;

    /// Upsert the persisted subscription for `subscription.user_id`. Touches
    /// `updated_at` on every call. Returns the row as actually stored.
    async fn upsert_user_subscription(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        subscription: &UserSubscription,
    ) -> Result<UserSubscription, UniversalInboxError>;

    /// Set or clear the over-limit grace deadline. `None` clears the column.
    async fn set_over_limit_grace_deadline(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        deadline: Option<DateTime<Utc>>,
    ) -> Result<(), UniversalInboxError>;

    /// Idempotency for Stripe webhook deliveries: records the event id and
    /// returns `true` if it was newly recorded (caller should process the
    /// event), `false` if it was already recorded (caller should skip).
    async fn record_stripe_event(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        event_id: &str,
        event_type: &str,
    ) -> Result<bool, UniversalInboxError>;

    /// Delete idempotency rows older than `days` so the `stripe_event` table
    /// doesn't grow unbounded. Returns the number of rows pruned. Intended for
    /// the reconcile cron.
    async fn prune_stripe_events_older_than_days(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        days: i64,
    ) -> Result<u64, UniversalInboxError>;

    /// Subscriptions whose persisted state might be stale and need a refresh
    /// from Stripe (e.g. nightly reconciliation job). Returns rows whose
    /// status is non-terminal.
    async fn list_subscriptions_needing_reconciliation(
        &self,
        executor: &mut Transaction<'_, Postgres>,
    ) -> Result<Vec<UserSubscription>, UniversalInboxError>;

    /// Rows whose over-limit grace deadline has passed, of ANY status. The
    /// grace-expiry pass must reach the terminal `canceled`/`unpaid` rows a
    /// real cancellation leaves behind — which the reconciliation query above
    /// deliberately skips — so it is keyed solely on a due deadline.
    async fn list_subscriptions_with_expired_grace_deadline(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        now: DateTime<Utc>,
    ) -> Result<Vec<UserSubscription>, UniversalInboxError>;
}

#[async_trait]
impl BillingRepository for Repository {
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string())
    )]
    async fn get_user_subscription(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Option<UserSubscription>, UniversalInboxError> {
        let row = sqlx::query(
            r#"
                SELECT user_id, stripe_customer_id, stripe_subscription_id, stripe_price_id,
                       plan, status, current_period_start, current_period_end,
                       cancel_at_period_end, canceled_at, over_limit_grace_deadline,
                       last_stripe_event_at, created_at, updated_at
                FROM user_subscription
                WHERE user_id = $1
            "#,
        )
        .bind(user_id.0)
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| UniversalInboxError::DatabaseError {
            message: format!("Failed to fetch user_subscription for user {user_id}: {err}"),
            source: err,
        })?;

        row.map(user_subscription_from_row).transpose()
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_user_subscription_by_customer_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        stripe_customer_id: &str,
    ) -> Result<Option<UserSubscription>, UniversalInboxError> {
        let row = sqlx::query(
            r#"
                SELECT user_id, stripe_customer_id, stripe_subscription_id, stripe_price_id,
                       plan, status, current_period_start, current_period_end,
                       cancel_at_period_end, canceled_at, over_limit_grace_deadline,
                       last_stripe_event_at, created_at, updated_at
                FROM user_subscription
                WHERE stripe_customer_id = $1
            "#,
        )
        .bind(stripe_customer_id)
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| UniversalInboxError::DatabaseError {
            message: format!(
                "Failed to fetch user_subscription for stripe_customer_id={stripe_customer_id}: {err}"
            ),
            source: err,
        })?;

        row.map(user_subscription_from_row).transpose()
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_user_subscription_by_subscription_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        stripe_subscription_id: &str,
    ) -> Result<Option<UserSubscription>, UniversalInboxError> {
        let row = sqlx::query(
            r#"
                SELECT user_id, stripe_customer_id, stripe_subscription_id, stripe_price_id,
                       plan, status, current_period_start, current_period_end,
                       cancel_at_period_end, canceled_at, over_limit_grace_deadline,
                       last_stripe_event_at, created_at, updated_at
                FROM user_subscription
                WHERE stripe_subscription_id = $1
            "#,
        )
        .bind(stripe_subscription_id)
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| UniversalInboxError::DatabaseError {
            message: format!(
                "Failed to fetch user_subscription for stripe_subscription_id={stripe_subscription_id}: {err}"
            ),
            source: err,
        })?;

        row.map(user_subscription_from_row).transpose()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = subscription.user_id.to_string())
    )]
    async fn upsert_user_subscription(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        subscription: &UserSubscription,
    ) -> Result<UserSubscription, UniversalInboxError> {
        let row = sqlx::query(
            r#"
            INSERT INTO user_subscription (
                user_id, stripe_customer_id, stripe_subscription_id, stripe_price_id,
                plan, status, current_period_start, current_period_end,
                cancel_at_period_end, canceled_at, over_limit_grace_deadline,
                last_stripe_event_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            ON CONFLICT (user_id) DO UPDATE SET
                stripe_customer_id = EXCLUDED.stripe_customer_id,
                stripe_subscription_id = EXCLUDED.stripe_subscription_id,
                stripe_price_id = EXCLUDED.stripe_price_id,
                plan = EXCLUDED.plan,
                status = EXCLUDED.status,
                current_period_start = EXCLUDED.current_period_start,
                current_period_end = EXCLUDED.current_period_end,
                cancel_at_period_end = EXCLUDED.cancel_at_period_end,
                canceled_at = EXCLUDED.canceled_at,
                over_limit_grace_deadline = EXCLUDED.over_limit_grace_deadline,
                last_stripe_event_at = EXCLUDED.last_stripe_event_at,
                updated_at = now()
            -- The write is its own ordering guard: `upsert_from_raw` checks
            -- the same condition against the row it read, which two concurrent
            -- webhooks can both pass. Here the comparison is against the
            -- committed row, so a stale event cannot land.
            -- A NULL on either side means "not an ordered subscription event"
            -- (customer linkage, `ensure_stripe_customer`, the grace
            -- placeholder); those carry the stored value forward and must
            -- still apply.
            WHERE user_subscription.last_stripe_event_at IS NULL
               OR EXCLUDED.last_stripe_event_at IS NULL
               OR user_subscription.last_stripe_event_at <= EXCLUDED.last_stripe_event_at
            RETURNING user_id, stripe_customer_id, stripe_subscription_id, stripe_price_id,
                      plan, status, current_period_start, current_period_end,
                      cancel_at_period_end, canceled_at, over_limit_grace_deadline,
                      last_stripe_event_at, created_at, updated_at
            "#,
        )
        .bind(subscription.user_id.0)
        .bind(subscription.stripe_customer_id.as_ref().map(|c| c.as_str()))
        .bind(
            subscription
                .stripe_subscription_id
                .as_ref()
                .map(|s| s.as_str()),
        )
        .bind(subscription.stripe_price_id.as_ref().map(|p| p.as_str()))
        .bind(subscription.plan.to_string())
        .bind(subscription.status.to_string())
        .bind(subscription.current_period_start)
        .bind(subscription.current_period_end)
        .bind(subscription.cancel_at_period_end)
        .bind(subscription.canceled_at)
        .bind(subscription.over_limit_grace_deadline)
        .bind(subscription.last_stripe_event_at)
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| UniversalInboxError::DatabaseError {
            message: format!(
                "Failed to upsert user_subscription for user {}: {err}",
                subscription.user_id
            ),
            source: err,
        })?;

        match row {
            Some(row) => user_subscription_from_row(row),
            // The ordering guard above filtered the update out: the stored row
            // was written from a newer Stripe event. Report the row that won,
            // so callers observe the state that is actually in force.
            None => {
                warn!(
                    "Skipped a stale user_subscription write for user {} (event {:?} is older than the stored one)",
                    subscription.user_id, subscription.last_stripe_event_at
                );
                self.get_user_subscription(executor, subscription.user_id)
                    .await?
                    .ok_or_else(|| {
                        UniversalInboxError::Unexpected(anyhow!(
                            "user_subscription for user {} vanished while upserting it",
                            subscription.user_id
                        ))
                    })
            }
        }
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string())
    )]
    async fn set_over_limit_grace_deadline(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        deadline: Option<DateTime<Utc>>,
    ) -> Result<(), UniversalInboxError> {
        sqlx::query(
            r#"
            UPDATE user_subscription
            SET over_limit_grace_deadline = $2,
                updated_at = now()
            WHERE user_id = $1
            "#,
        )
        .bind(user_id.0)
        .bind(deadline)
        .execute(&mut **executor)
        .await
        .map_err(|err| UniversalInboxError::DatabaseError {
            message: format!("Failed to update grace deadline for user {user_id}: {err}"),
            source: err,
        })?;
        Ok(())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::STRIPE_EVENT_ID } = event_id)
    )]
    async fn record_stripe_event(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        event_id: &str,
        event_type: &str,
    ) -> Result<bool, UniversalInboxError> {
        // ON CONFLICT DO NOTHING + RETURNING is the canonical "insert-if-new"
        // pattern: zero rows returned ⇒ the row already existed.
        let row = sqlx::query(
            r#"
            INSERT INTO stripe_event (event_id, event_type)
            VALUES ($1, $2)
            ON CONFLICT (event_id) DO NOTHING
            RETURNING event_id
            "#,
        )
        .bind(event_id)
        .bind(event_type)
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| UniversalInboxError::DatabaseError {
            message: format!("Failed to record stripe_event {event_id}: {err}"),
            source: err,
        })?;
        Ok(row.is_some())
    }

    #[tracing::instrument(level = "debug", skip_all, fields({ attr::STRIPE_EVENT_RETENTION_DAYS } = days))]
    async fn prune_stripe_events_older_than_days(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        days: i64,
    ) -> Result<u64, UniversalInboxError> {
        let result = sqlx::query(
            r#"
            DELETE FROM stripe_event
            WHERE received_at < now() - make_interval(days => $1)
            "#,
        )
        .bind(days as i32)
        .execute(&mut **executor)
        .await
        .map_err(|err| UniversalInboxError::DatabaseError {
            message: format!("Failed to prune stripe_event rows older than {days} days: {err}"),
            source: err,
        })?;
        Ok(result.rows_affected())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_subscriptions_needing_reconciliation(
        &self,
        executor: &mut Transaction<'_, Postgres>,
    ) -> Result<Vec<UserSubscription>, UniversalInboxError> {
        // Pull every row that *might* still change in Stripe. Terminal rows
        // (canceled / unpaid / incomplete_expired) are excluded — they don't
        // come back without a brand-new subscription — *unless* the row knows a
        // customer but no subscription: that shape is what a missed
        // `checkout.session.completed` webhook leaves behind, and the customer
        // may well hold a live subscription Stripe can hand back. Terminal
        // status is no argument against it, since a user who cancels and then
        // re-subscribes leaves exactly that row.
        let rows = sqlx::query(
            r#"
            SELECT user_id, stripe_customer_id, stripe_subscription_id, stripe_price_id,
                   plan, status, current_period_start, current_period_end,
                   cancel_at_period_end, canceled_at, over_limit_grace_deadline,
                   last_stripe_event_at, created_at, updated_at
            FROM user_subscription
            WHERE status NOT IN ('canceled', 'unpaid', 'incomplete_expired')
               OR (stripe_customer_id IS NOT NULL AND stripe_subscription_id IS NULL)
            "#,
        )
        .fetch_all(&mut **executor)
        .await
        .map_err(|err| UniversalInboxError::DatabaseError {
            message: format!("Failed to list subscriptions for reconciliation: {err}"),
            source: err,
        })?;

        rows.into_iter().map(user_subscription_from_row).collect()
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_subscriptions_with_expired_grace_deadline(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        now: DateTime<Utc>,
    ) -> Result<Vec<UserSubscription>, UniversalInboxError> {
        let rows = sqlx::query(
            r#"
            SELECT user_id, stripe_customer_id, stripe_subscription_id, stripe_price_id,
                   plan, status, current_period_start, current_period_end,
                   cancel_at_period_end, canceled_at, over_limit_grace_deadline,
                   last_stripe_event_at, created_at, updated_at
            FROM user_subscription
            WHERE over_limit_grace_deadline IS NOT NULL
              AND over_limit_grace_deadline <= $1
            "#,
        )
        .bind(now)
        .fetch_all(&mut **executor)
        .await
        .map_err(|err| UniversalInboxError::DatabaseError {
            message: format!("Failed to list subscriptions with expired grace deadline: {err}"),
            source: err,
        })?;

        rows.into_iter().map(user_subscription_from_row).collect()
    }
}

fn user_subscription_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<UserSubscription, UniversalInboxError> {
    let user_id: uuid::Uuid = row.try_get("user_id").map_err(database_err)?;
    let stripe_customer_id: Option<String> =
        row.try_get("stripe_customer_id").map_err(database_err)?;
    let stripe_subscription_id: Option<String> = row
        .try_get("stripe_subscription_id")
        .map_err(database_err)?;
    let stripe_price_id: Option<String> = row.try_get("stripe_price_id").map_err(database_err)?;
    let plan_str: String = row.try_get("plan").map_err(database_err)?;
    let status_str: String = row.try_get("status").map_err(database_err)?;
    let current_period_start: Option<DateTime<Utc>> =
        row.try_get("current_period_start").map_err(database_err)?;
    let current_period_end: Option<DateTime<Utc>> =
        row.try_get("current_period_end").map_err(database_err)?;
    let cancel_at_period_end: bool = row.try_get("cancel_at_period_end").map_err(database_err)?;
    let canceled_at: Option<DateTime<Utc>> = row.try_get("canceled_at").map_err(database_err)?;
    let over_limit_grace_deadline: Option<DateTime<Utc>> = row
        .try_get("over_limit_grace_deadline")
        .map_err(database_err)?;
    let last_stripe_event_at: Option<DateTime<Utc>> =
        row.try_get("last_stripe_event_at").map_err(database_err)?;
    let created_at: DateTime<Utc> = row.try_get("created_at").map_err(database_err)?;
    let updated_at: DateTime<Utc> = row.try_get("updated_at").map_err(database_err)?;

    Ok(UserSubscription {
        user_id: UserId(user_id),
        stripe_customer_id: stripe_customer_id.map(StripeCustomerId),
        stripe_subscription_id: stripe_subscription_id.map(StripeSubscriptionId),
        stripe_price_id: stripe_price_id.map(StripePriceId),
        plan: plan_str.parse::<Plan>().map_err(|_| {
            UniversalInboxError::Unexpected(anyhow!(
                "Unknown plan value in user_subscription: {plan_str}"
            ))
        })?,
        status: status_str.parse::<SubscriptionStatus>().map_err(|_| {
            UniversalInboxError::Unexpected(anyhow!(
                "Unknown subscription status in user_subscription: {status_str}"
            ))
        })?,
        current_period_start,
        current_period_end,
        cancel_at_period_end,
        canceled_at,
        over_limit_grace_deadline,
        last_stripe_event_at,
        created_at,
        updated_at,
    })
}

fn database_err(err: sqlx::Error) -> UniversalInboxError {
    UniversalInboxError::DatabaseError {
        message: format!("Failed to read user_subscription row: {err}"),
        source: err,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plan_round_trip() {
        assert_eq!("free".parse::<Plan>().unwrap(), Plan::Free);
        assert_eq!("paid".parse::<Plan>().unwrap(), Plan::Paid);
        assert!("enterprise".parse::<Plan>().is_err());
    }

    #[test]
    fn parse_status_round_trip() {
        assert_eq!(
            "active".parse::<SubscriptionStatus>().unwrap(),
            SubscriptionStatus::Active
        );
        assert_eq!(
            "past_due".parse::<SubscriptionStatus>().unwrap(),
            SubscriptionStatus::PastDue
        );
        assert_eq!(
            "incomplete_expired".parse::<SubscriptionStatus>().unwrap(),
            SubscriptionStatus::IncompleteExpired
        );
        assert!("unknown".parse::<SubscriptionStatus>().is_err());
    }

    #[test]
    fn parse_status_matches_display() {
        // Every variant of SubscriptionStatus must Display to a string that
        // its FromStr can decode — guards the DB round-trip.
        for status in [
            SubscriptionStatus::Active,
            SubscriptionStatus::PastDue,
            SubscriptionStatus::Canceled,
            SubscriptionStatus::Unpaid,
            SubscriptionStatus::Incomplete,
            SubscriptionStatus::IncompleteExpired,
            SubscriptionStatus::Trialing,
            SubscriptionStatus::Paused,
        ] {
            let s = status.to_string();
            assert_eq!(
                s.parse::<SubscriptionStatus>().unwrap(),
                status,
                "round-trip failed for {status:?} (rendered as {s:?})"
            );
        }
    }
}
