//! Plan resolution, limit enforcement, and Stripe state synchronisation.
//!
//! `BillingService` is the single entry point used by the rest of the API.
//! When the operator omits the `[billing]` configuration block, the service
//! is never instantiated — call sites guard with `Option<Arc<BillingService>>`
//! and fall through to "Free behaviour preserved" (which on those instances
//! means *unlimited* behaviour preserved).

use std::sync::Arc;

use anyhow::anyhow;
use chrono::{DateTime, TimeDelta, Utc};
use serde_json::json;
use sqlx::{Postgres, Transaction};
use tracing::{debug, info, warn};
use universal_inbox::{
    billing::{
        BillingLimits, FREE_PLAN_INTEGRATION_LIMIT_CODE, Plan, StripeCustomerId, StripePriceId,
        StripeSubscriptionId, UserSubscription,
    },
    integration_connection::provider::IntegrationProviderKind,
    user::{User, UserId},
};
use url::Url;

use crate::observability::RecordSpanError;
use crate::observability::attr;
use crate::{
    billing::{
        repository::BillingRepository,
        stripe::{
            CheckoutSessionParams, PortalSessionParams, RawSubscription, StripeClient, StripeEvent,
            StripeEventKind,
        },
    },
    configuration::FreePlanSettings,
    repository::{Repository, user::UserRepository},
    universal_inbox::UniversalInboxError,
};

/// Retention window for the Stripe webhook idempotency table (`stripe_event`).
/// Rows older than this are pruned by the reconcile cron.
pub const STRIPE_EVENT_RETENTION_DAYS: i64 = 30;

/// What one `billing reconcile` run changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReconcileReport {
    /// Rows whose known subscription was re-read from Stripe.
    pub refreshed: usize,
    /// Rows that only knew their customer and got their subscription linked
    /// from Stripe — one per webhook the instance never received.
    pub adopted: usize,
    /// Rows whose over-limit grace deadline came due and was enforced.
    pub grace_expired: usize,
}

/// Trait-erased provider for "how many validated integration connections does
/// `user_id` have right now." Kept as a trait so the service can be wired up
/// without a hard dependency on `IntegrationConnectionService` (and so tests
/// can stub it directly).
#[async_trait::async_trait]
pub trait IntegrationConnectionCounter: Send + Sync {
    async fn count_validated_for_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<u32, UniversalInboxError>;

    /// How many of `user_id`'s connections the plan has paused. Not part of
    /// the cap arithmetic — paused connections consume no slot — but the UI
    /// needs the number to explain why integrations stopped syncing.
    async fn count_plan_paused_for_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<u32, UniversalInboxError>;
}

/// Concrete `IntegrationConnectionCounter` backed by the shared `Repository`.
/// Used at boot to wire up `BillingService` without creating an Arc cycle with
/// the IntegrationConnectionService (which itself holds an
/// `Arc<BillingService>`).
pub struct RepositoryIntegrationCounter {
    pub repository: Arc<Repository>,
}

#[async_trait::async_trait]
impl IntegrationConnectionCounter for RepositoryIntegrationCounter {
    async fn count_validated_for_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<u32, UniversalInboxError> {
        use crate::repository::integration_connection::IntegrationConnectionRepository;
        self.repository
            .count_validated_integration_connections(executor, user_id)
            .await
    }

    async fn count_plan_paused_for_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<u32, UniversalInboxError> {
        use crate::repository::integration_connection::IntegrationConnectionRepository;
        self.repository
            .count_plan_paused_integration_connections(executor, user_id)
            .await
    }
}

pub struct BillingService {
    pub(crate) repository: Arc<Repository>,
    pub(crate) stripe: Arc<dyn StripeClient>,
    pub(crate) limits: BillingLimits,
    pub(crate) stripe_price_id: StripePriceId,
    pub(crate) integration_counter: Arc<dyn IntegrationConnectionCounter>,
}

impl BillingService {
    pub fn new(
        repository: Arc<Repository>,
        stripe: Arc<dyn StripeClient>,
        free_plan: &FreePlanSettings,
        stripe_price_id: StripePriceId,
        integration_counter: Arc<dyn IntegrationConnectionCounter>,
    ) -> Self {
        Self {
            repository,
            stripe,
            limits: BillingLimits {
                max_integration_connections: free_plan.max_integration_connections,
                notification_sync_interval_in_minutes: free_plan
                    .notification_sync_interval_in_minutes,
                task_sync_interval_in_minutes: free_plan.task_sync_interval_in_minutes,
                rollout_grace_days: free_plan.rollout_grace_days,
            },
            stripe_price_id,
            integration_counter,
        }
    }

    pub fn limits(&self) -> &BillingLimits {
        &self.limits
    }

    /// Prune `stripe_event` idempotency rows older than
    /// [`STRIPE_EVENT_RETENTION_DAYS`]. No-op (returns 0) under `dry_run`.
    /// Called from the reconcile cron so the table can't grow unbounded.
    pub async fn prune_stripe_events(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        dry_run: bool,
    ) -> Result<u64, UniversalInboxError> {
        if dry_run {
            return Ok(0);
        }
        self.repository
            .prune_stripe_events_older_than_days(executor, STRIPE_EVENT_RETENTION_DAYS)
            .await
    }

    pub async fn begin(&self) -> Result<Transaction<'_, Postgres>, UniversalInboxError> {
        self.repository.begin().await
    }

    /// Run one unit of work in a transaction of its own, committing it before
    /// returning.
    async fn write_in_own_transaction<F>(&self, write: F) -> Result<(), UniversalInboxError>
    where
        F: AsyncFnOnce(&mut Transaction<'_, Postgres>) -> Result<(), UniversalInboxError>,
    {
        let mut transaction = self.begin().await?;
        write(&mut transaction).await?;
        transaction.commit().await.map_err(|err| {
            UniversalInboxError::Unexpected(anyhow!("Failed to commit a reconcile write: {err}"))
        })
    }

    /// Resolves the effective plan for a user. Absence of a row ⇒ `Free`.
    pub async fn get_user_plan(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Plan, UniversalInboxError> {
        match self
            .repository
            .get_user_subscription(executor, user_id)
            .await?
        {
            Some(sub) => Ok(sub.effective_plan()),
            None => Ok(Plan::Free),
        }
    }

    /// Returns the effective minimum sync interval for `kind`, taking the
    /// *more restrictive* of the global config floor and the plan-specific
    /// floor (for Free users). Paid users see the global floor unchanged.
    /// Single `SyncKind`-parameterized method replacing the former
    /// notification/task twins.
    pub async fn effective_sync_interval(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        global_floor_in_minutes: i64,
        kind: SyncKind,
    ) -> Result<i64, UniversalInboxError> {
        let plan = self.get_user_plan(executor, user_id).await?;
        Ok(effective_sync_interval(
            &self.limits,
            plan,
            global_floor_in_minutes,
            kind,
        ))
    }

    /// The minimum sync interval the scheduler must honour for `kind`, given
    /// whether this run is a user-requested `force_sync`. Collapses the twin
    /// force-sync match blocks the notification and task services used to
    /// carry:
    /// - Paid + force ⇒ `0` (sync now);
    /// - Free + force ⇒ still respect the effective floor `max(global, plan)`
    ///   so force-sync can't silently undercut a stricter global floor;
    /// - not forced ⇒ the plan's effective interval.
    pub async fn min_sync_interval(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        global_floor_in_minutes: i64,
        kind: SyncKind,
        force_sync: bool,
    ) -> Result<i64, UniversalInboxError> {
        let plan = self.get_user_plan(executor, user_id).await?;
        Ok(match (force_sync, plan.is_paid()) {
            (true, true) => 0,
            (true, false) => {
                effective_sync_interval(&self.limits, Plan::Free, global_floor_in_minutes, kind)
            }
            (false, _) => {
                effective_sync_interval(&self.limits, plan, global_floor_in_minutes, kind)
            }
        })
    }

    /// Enforce the Free-plan integration cap. Returns `Err(PaymentRequired)`
    /// when the user is Free and already at-or-above the limit. Test users
    /// (`User::is_testing = true`) are exempt so the
    /// `just api generate-user` seeder can keep populating 6+ integrations.
    ///
    /// The implicit `API` connection is excluded from the count this cap is
    /// measured against, so it is not refused by it either.
    pub async fn assert_can_add_integration(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        provider_kind: IntegrationProviderKind,
    ) -> Result<(), UniversalInboxError> {
        if provider_kind == IntegrationProviderKind::API {
            return Ok(());
        }

        // Serialize the count-then-insert against parallel OAuth callbacks for
        // the same user so two below-cap checks can't both pass and overshoot
        // the Free limit. The lock is held until this request's transaction
        // (which writes the new Validated row) commits.
        self.repository
            .acquire_user_advisory_lock(executor, user_id)
            .await?;

        if let Some(user) = self.repository.get_user(executor, user_id).await?
            && user.is_testing
        {
            return Ok(());
        }

        let plan = self.get_user_plan(executor, user_id).await?;
        if plan.is_paid() {
            return Ok(());
        }

        let current = self
            .integration_counter
            .count_validated_for_user(executor, user_id)
            .await?;
        let limit = self.limits.max_integration_connections;
        if current < limit {
            return Ok(());
        }

        Err(UniversalInboxError::PaymentRequired {
            code: FREE_PLAN_INTEGRATION_LIMIT_CODE,
            message: format!(
                "Free plan allows at most {limit} integration connection{plural}. \
                 Upgrade to keep adding integrations.",
                plural = if limit == 1 { "" } else { "s" }
            ),
            details: json!({
                "current_plan": "free",
                "limit": limit,
                "usage": current,
            }),
        })
    }

    /// Create-or-reuse the Stripe Customer for `user_id` and start a
    /// Checkout session. Returns the hosted URL the UI should redirect to.
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields({ attr::USER_ID } = %user_id, { attr::ERROR_TYPE } = tracing::field::Empty)
    )]
    pub async fn create_checkout_session(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        success_url: Url,
        cancel_url: Url,
    ) -> Result<Url, UniversalInboxError> {
        let result: Result<Url, UniversalInboxError> = async move {
            let user = self
                .repository
                .get_user(executor, user_id)
                .await?
                .ok_or_else(|| {
                    UniversalInboxError::ItemNotFound(format!("User {user_id} not found"))
                })?;

            let customer_id = self.ensure_stripe_customer(executor, &user).await?;

            let url = self
                .stripe
                .create_checkout_session(CheckoutSessionParams {
                    customer_id,
                    price_id: self.stripe_price_id.clone(),
                    success_url,
                    cancel_url,
                    user_id,
                })
                .await?;
            Ok(url)
        }
        .await;
        result.record_span_error()
    }

    /// Start a Stripe Customer Portal session for an existing customer.
    /// Returns the hosted URL.
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields({ attr::USER_ID } = %user_id, { attr::ERROR_TYPE } = tracing::field::Empty)
    )]
    pub async fn create_portal_session(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        return_url: Url,
    ) -> Result<Url, UniversalInboxError> {
        let result: Result<Url, UniversalInboxError> = async move {
            let no_customer = || UniversalInboxError::PaymentRequired {
                code: "no_stripe_customer",
                message: "Start a paid subscription first to access the billing portal."
                    .to_string(),
                details: json!({ "current_plan": "free" }),
            };
            let customer_id = self
                .repository
                .get_user_subscription(executor, user_id)
                .await?
                .and_then(|sub| sub.stripe_customer_id)
                .ok_or_else(no_customer)?;

            let url = self
                .stripe
                .create_portal_session(PortalSessionParams {
                    customer_id,
                    return_url,
                })
                .await?;
            Ok(url)
        }
        .await;
        result.record_span_error()
    }

    /// Stop billing a user whose account is being deleted. Must run *before*
    /// the local `user_subscription` row is deleted, so a Stripe failure
    /// aborts the account deletion instead of leaving an orphaned subscription
    /// that keeps charging the (deleted) user.
    ///
    /// - Every non-final subscription of the user's customer is cancelled
    ///   immediately: the one the local row knows, plus any Stripe holds for
    ///   the customer that never reached us (missed webhook). A cancel that
    ///   fails is re-read from Stripe and only tolerated if the subscription is
    ///   already canceled (e.g. cancelled from the portal, webhook in flight).
    /// - The Stripe Customer is kept, with its invoices (10-year accounting
    ///   retention), but its `metadata.user_id` link is removed. That part is
    ///   best effort: it does not charge anyone, so a failure is only logged.
    ///
    /// No-op for a user without a `user_subscription` row (Free user who never
    /// started a checkout).
    #[tracing::instrument(level = "info", skip_all, fields({ attr::USER_ID } = user_id.to_string(), { attr::ERROR_TYPE } = tracing::field::Empty))]
    pub async fn cancel_billing_for_account_deletion(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let result: Result<(), UniversalInboxError> = async move {
        let Some(subscription) = self
            .repository
            .get_user_subscription(executor, user_id)
            .await?
        else {
            return Ok(());
        };

        let mut subscription_ids: Vec<String> = Vec::new();
        if let Some(subscription_id) = &subscription.stripe_subscription_id
            && still_billable(subscription.status)
        {
            subscription_ids.push(subscription_id.0.clone());
        }
        if let Some(customer_id) = &subscription.stripe_customer_id {
            let remote = self
                .stripe
                .list_customer_subscriptions(&customer_id.0)
                .await?;
            for raw in remote {
                if still_billable(raw.status) && !subscription_ids.contains(&raw.subscription_id) {
                    subscription_ids.push(raw.subscription_id);
                }
            }
        }

        for subscription_id in subscription_ids {
            match self.stripe.cancel_subscription(&subscription_id).await {
                Ok(raw) => info!(
                    "Cancelled Stripe subscription {subscription_id} ({}) of deleted user {user_id}",
                    raw.status
                ),
                Err(cancel_err) => match self.stripe.fetch_subscription(&subscription_id).await {
                    Ok(raw) if !still_billable(raw.status) => info!(
                        "Stripe subscription {subscription_id} of deleted user {user_id} is already {}",
                        raw.status
                    ),
                    _ => {
                        return Err(UniversalInboxError::from(cancel_err));
                    }
                },
            }
        }

        if let Some(customer_id) = &subscription.stripe_customer_id
            && let Err(err) = self.stripe.clear_customer_user_id(&customer_id.0).await
        {
            warn!(
                "Failed to remove the user_id metadata from Stripe customer {customer_id} of deleted user {user_id}: {err}"
            );
        }

        Ok(())
    }.await;
        result.record_span_error()
    }

    /// Apply an incoming, already-verified webhook event to the persisted
    /// subscription. Idempotency is enforced by the caller via
    /// `BillingRepository::record_stripe_event` before invoking this.
    ///
    /// `prefetched` carries the `RawSubscription` the webhook handler fetched
    /// from Stripe *before* opening the transaction (for the event kinds that
    /// need a live fetch — checkout-with-subscription, invoice paid/failed).
    /// Keeping the network call out of the transaction is what prevents a
    /// Stripe latency spike from pinning a DB connection per delivery.
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields({ attr::ERROR_TYPE } = tracing::field::Empty)
    )]
    pub async fn apply_subscription_event(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        event: &StripeEvent,
        prefetched: Option<RawSubscription>,
    ) -> Result<(), UniversalInboxError> {
        let result: Result<(), UniversalInboxError> = async move {
            match &event.kind {
                StripeEventKind::CheckoutSessionCompleted {
                    customer_id,
                    subscription_id,
                    user_id_metadata,
                } => {
                    self.handle_checkout_completed(
                        executor,
                        customer_id.as_deref(),
                        subscription_id.as_deref(),
                        *user_id_metadata,
                        prefetched,
                        event.created,
                    )
                    .await
                }
                StripeEventKind::SubscriptionUpserted(raw) => {
                    self.upsert_from_raw(executor, raw, None, event.created)
                        .await
                }
                StripeEventKind::SubscriptionDeleted(raw) => {
                    // At end-of-period the user falls back to Free. `upsert_from_raw`
                    // detects the Paid → Free transition and arms the grace deadline;
                    // the reconcile job pauses excess connections after it expires
                    // (uniform grace — no immediate pause here).
                    self.upsert_from_raw(executor, raw, None, event.created)
                        .await
                }
                StripeEventKind::InvoicePaid { .. }
                | StripeEventKind::InvoicePaymentFailed { .. } => {
                    // The webhook handler prefetched the subscription state when an
                    // invoice carried a subscription id; nothing to do otherwise.
                    if let Some(raw) = prefetched {
                        self.upsert_from_raw(executor, &raw, None, event.created)
                            .await
                    } else {
                        Ok(())
                    }
                }
                StripeEventKind::Other => Ok(()),
            }
        }
        .await;
        result.record_span_error()
    }

    /// Resolve the local subscription row for a Checkout completion. When the
    /// session already carries a subscription, the webhook handler fetched its
    /// fresh state (`prefetched`) before opening the transaction; otherwise we
    /// record the customer linkage so subsequent subscription events resolve.
    async fn handle_checkout_completed(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        customer_id: Option<&str>,
        subscription_id: Option<&str>,
        user_id_metadata: Option<UserId>,
        prefetched: Option<RawSubscription>,
        event_created: DateTime<Utc>,
    ) -> Result<(), UniversalInboxError> {
        let user_id = match user_id_metadata {
            Some(u) => u,
            None => {
                warn!(
                    "checkout.session.completed missing user_id metadata (customer={customer_id:?}, subscription={subscription_id:?}); skipping"
                );
                return Ok(());
            }
        };

        if let Some(raw) = prefetched {
            // Subscription already attached and prefetched outside the txn.
            // Pass the metadata user id as the fallback so the row is created
            // and linked even when no prior row matches the customer (race,
            // DB restore, duplicate customer) — otherwise the user is charged
            // but stays Free.
            self.upsert_from_raw(executor, &raw, Some(user_id), event_created)
                .await
        } else if let Some(cust_id) = customer_id {
            // Subscription not yet attached on the session; record the
            // customer linkage so subsequent subscription events resolve.
            // Deliberately does NOT touch `last_stripe_event_at` — this is
            // customer linkage, not subscription state, so a subsequent
            // `subscription.created` (which may carry an earlier `created`)
            // must not be guarded out as stale.
            let now = Utc::now();
            let row = self
                .repository
                .get_user_subscription(executor, user_id)
                .await?;
            // Customer linkage and nothing else: spelling the other fields
            // out here would downgrade a Paid row, and the ordering guard
            // cannot catch it since this write carries the stored watermark
            // forward.
            let to_save = UserSubscription {
                stripe_customer_id: Some(StripeCustomerId(cust_id.to_string())),
                ..UserSubscription::carry_forward(user_id, row.as_ref(), now)
            };
            self.repository
                .upsert_user_subscription(executor, &to_save)
                .await?;
            Ok(())
        } else {
            warn!("checkout.session.completed missing both customer and subscription ids");
            Ok(())
        }
    }

    /// Persist a `RawSubscription`, resolving the local `user_id` through the
    /// existing subscription/customer row, falling back to `fallback_user_id`
    /// (the `metadata.user_id` Stripe carries). `event_created` is the source
    /// event's timestamp; rows already advanced past it are left untouched so
    /// out-of-order Stripe deliveries can't resurrect stale state.
    async fn upsert_from_raw(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        raw: &RawSubscription,
        fallback_user_id: Option<UserId>,
        event_created: DateTime<Utc>,
    ) -> Result<(), UniversalInboxError> {
        let existing_by_sub = self
            .repository
            .get_user_subscription_by_subscription_id(executor, &raw.subscription_id)
            .await?;
        let existing_by_customer = match existing_by_sub {
            Some(_) => None,
            None => {
                self.repository
                    .get_user_subscription_by_customer_id(executor, &raw.customer_id)
                    .await?
            }
        };

        // Resolve the user. Prefer an existing row (by subscription, then
        // customer); fall back to the trusted `metadata.user_id` and reuse any
        // row already keyed on that user (e.g. a pending checkout placeholder)
        // so its created_at / grace deadline survive.
        let (user_id, existing) = match existing_by_sub.or(existing_by_customer) {
            Some(found) => (found.user_id, Some(found)),
            None => match fallback_user_id {
                Some(uid) => (
                    uid,
                    self.repository.get_user_subscription(executor, uid).await?,
                ),
                None => {
                    warn!(
                        "Stripe subscription {} references unknown customer {} and no user_id metadata fallback; skipping",
                        raw.subscription_id, raw.customer_id
                    );
                    return Ok(());
                }
            },
        };

        // Ordering guard: ignore events older than the last one applied to
        // this row. Stripe does not guarantee delivery order, so a retried
        // stale `subscription.updated(active)` can arrive after
        // `subscription.deleted` and would otherwise resurrect a canceled sub.
        if let Some(existing) = existing.as_ref()
            && let Some(last_applied) = existing.last_stripe_event_at
            && event_created < last_applied
        {
            warn!(
                "Ignoring stale Stripe event for subscription {} (created {event_created} < last applied {last_applied})",
                raw.subscription_id
            );
            return Ok(());
        }

        let now = Utc::now();
        let plan = if raw.status.is_paid_entitled() {
            Plan::Paid
        } else {
            Plan::Free
        };

        let updated = UserSubscription {
            user_id,
            stripe_customer_id: Some(StripeCustomerId(raw.customer_id.clone())),
            stripe_subscription_id: Some(StripeSubscriptionId(raw.subscription_id.clone())),
            stripe_price_id: raw.price_id.clone().map(StripePriceId),
            plan,
            status: raw.status,
            current_period_start: raw.current_period_start,
            current_period_end: raw.current_period_end,
            cancel_at_period_end: raw.cancel_at_period_end,
            canceled_at: raw.canceled_at,
            over_limit_grace_deadline: existing.as_ref().and_then(|s| s.over_limit_grace_deadline),
            last_stripe_event_at: Some(event_created),
            created_at: existing.as_ref().map(|s| s.created_at).unwrap_or(now),
            updated_at: now,
        };

        // Detect the entitlement transition against the prior row BEFORE the
        // upsert overwrote it (`existing` is the pre-upsert snapshot). This is
        // the single chokepoint every subscription event funnels through, so it
        // covers cancel-via-update, deletion, and re-activation uniformly.
        let was_paid = existing
            .as_ref()
            .map(|s| s.effective_plan().is_paid())
            .unwrap_or(false);

        // The write carries its own ordering guard, so it may be filtered out
        // by an event that won a race, and it answers with the row actually in
        // force. The transition below follows that row: a rejected write must
        // not clear the grace deadline or restore paused connections.
        let applied = self
            .repository
            .upsert_user_subscription(executor, &updated)
            .await?;
        let now_paid = applied.effective_plan().is_paid();

        if was_paid && !now_paid {
            // Paid → Free: arm the grace clock. The reconcile job performs the
            // actual pause once the deadline passes (uniform grace — explicit
            // cancel and rollout over-limit behave identically).
            self.mark_over_limit_grace_deadline_if_needed(executor, user_id)
                .await?;
        } else if !was_paid && now_paid {
            // Free → Paid: clear any pending grace deadline and restore the
            // connections we plan-paused, back to their pre-pause config.
            self.repository
                .set_over_limit_grace_deadline(executor, user_id, None)
                .await?;
            self.restore_plan_paused_connections(executor, user_id)
                .await?;
        }
        Ok(())
    }

    /// Sets `over_limit_grace_deadline = now + rollout_grace_days` when the
    /// user is Free *and* currently over-limit *and* no deadline is recorded
    /// yet. Idempotent: returns immediately on subsequent calls.
    ///
    /// Symmetrically *clears* a previously-armed deadline once the user is back
    /// at/under the cap, so the over-limit banner and billing-page warning stop
    /// showing the moment they disconnect down to compliance (or the usage count
    /// is corrected). Safe because a non-null deadline always means "grace not
    /// yet expired" — expiry (`reconcile_subscriptions`) clears the deadline in
    /// the same step that it pauses the excess connections — so there are never
    /// plan-paused connections to restore on this path.
    pub async fn mark_over_limit_grace_deadline_if_needed(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Option<DateTime<Utc>>, UniversalInboxError> {
        let plan = self.get_user_plan(executor, user_id).await?;
        if plan.is_paid() {
            return Ok(None);
        }

        let current = self
            .integration_counter
            .count_validated_for_user(executor, user_id)
            .await?;
        if current <= self.limits.max_integration_connections {
            // Back under the cap — cancel any stale countdown.
            if let Some(sub) = self
                .repository
                .get_user_subscription(executor, user_id)
                .await?
                && sub.over_limit_grace_deadline.is_some()
            {
                self.repository
                    .set_over_limit_grace_deadline(executor, user_id, None)
                    .await?;
            }
            return Ok(None);
        }

        let existing = self
            .repository
            .get_user_subscription(executor, user_id)
            .await?;
        if let Some(sub) = &existing
            && sub.over_limit_grace_deadline.is_some()
        {
            return Ok(sub.over_limit_grace_deadline);
        }

        let deadline = Utc::now()
            + TimeDelta::try_days(self.limits.rollout_grace_days as i64).ok_or_else(|| {
                UniversalInboxError::Unexpected(anyhow!(
                    "rollout_grace_days produces overflowing TimeDelta"
                ))
            })?;

        // If no row exists yet (no Stripe customer), create a Free placeholder
        // with the deadline. The customer id is filled in when checkout starts.
        match existing {
            Some(_) => {
                self.repository
                    .set_over_limit_grace_deadline(executor, user_id, Some(deadline))
                    .await?;
            }
            None => {
                let now = Utc::now();
                // No row yet, so this is a fresh Free placeholder that exists
                // only to carry the grace deadline. No Stripe customer either
                // — that is filled in when checkout starts.
                let placeholder = UserSubscription {
                    over_limit_grace_deadline: Some(deadline),
                    ..UserSubscription::carry_forward(user_id, None, now)
                };
                self.repository
                    .upsert_user_subscription(executor, &placeholder)
                    .await?;
            }
        }
        Ok(Some(deadline))
    }

    /// Walk every subscription that might still change, refresh its state from
    /// Stripe, adopt subscriptions whose webhook never arrived, and expire any
    /// over_limit_grace_deadline that has passed. Intended for the daily
    /// `universal-inbox-api billing reconcile` CLI subcommand.
    /// Each row is its own transaction, opened after Stripe has answered, so
    /// nothing is held open across a network round trip and one row's failure
    /// cannot roll back the rows already reconciled.
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields({ attr::ERROR_TYPE } = tracing::field::Empty)
    )]
    pub async fn reconcile_subscriptions(
        &self,
        dry_run: bool,
    ) -> Result<ReconcileReport, UniversalInboxError> {
        let result: Result<ReconcileReport, UniversalInboxError> = async move {
        use crate::billing::repository::BillingRepository;
        let mut executor = self.begin().await?;
        let subs = self
            .repository
            .list_subscriptions_needing_reconciliation(&mut executor)
            .await?;
        executor.commit().await.map_err(|err| {
            UniversalInboxError::Unexpected(anyhow!(
                "Failed to commit the reconciliation listing: {err}"
            ))
        })?;

        let mut refreshed = 0usize;
        let mut adopted = 0usize;
        let mut grace_expired = 0usize;
        let now = Utc::now();

        // Pass 1 — refresh rows that already know their subscription, and adopt
        // the ones that don't.
        for sub in &subs {
            match (&sub.stripe_subscription_id, &sub.stripe_customer_id) {
                (Some(sub_id), _) => match self.stripe.fetch_subscription(sub_id.as_str()).await {
                    Ok(raw) => {
                        if !dry_run {
                            // Reconciliation is a deliberate refresh, not an
                            // ordered webhook delivery: no metadata fallback,
                            // and `now()` so the freshly-fetched state is never
                            // rejected by the ordering guard.
                            if let Err(err) = self
                                .write_in_own_transaction(async |tx| {
                                    self.upsert_from_raw(tx, &raw, None, Utc::now()).await
                                })
                                .await
                            {
                                warn!(
                                    "Refresh of subscription {sub_id} failed to save (skipping): {err}"
                                );
                                continue;
                            }
                        }
                        refreshed += 1;
                    }
                    Err(err) => {
                        // Log loudly but don't fail the batch: a missing user
                        // or subscription must not stop the other rows from
                        // being reconciled.
                        warn!("Stripe fetch for subscription {sub_id} failed (skipping): {err}");
                    }
                },
                // Customer linked at checkout but no subscription id: the
                // `checkout.session.completed` webhook never landed (dropped
                // delivery, no local forwarder, endpoint down), leaving a
                // charged user on Free. Nothing else repairs that, so ask
                // Stripe what the customer actually holds.
                (None, Some(customer_id)) => {
                    let candidates = match self
                        .stripe
                        .list_customer_subscriptions(customer_id.as_str())
                        .await
                    {
                        Ok(candidates) => candidates,
                        Err(err) => {
                            warn!(
                                "Stripe subscription listing for customer {customer_id} failed (skipping): {err}"
                            );
                            continue;
                        }
                    };
                    let Some(raw) = candidates.iter().find(|raw| !raw.status.is_terminal()) else {
                        debug!(
                            "Stripe customer {customer_id} (user {}) holds no live subscription to adopt",
                            sub.user_id
                        );
                        continue;
                    };
                    if !dry_run {
                        // The row is keyed on the user already, so pass it as
                        // the fallback: no row matches this subscription id
                        // yet, and the customer lookup is what we are
                        // repairing.
                        if let Err(err) = self
                            .write_in_own_transaction(async |tx| {
                                self.upsert_from_raw(tx, raw, Some(sub.user_id), Utc::now())
                                    .await
                            })
                            .await
                        {
                            warn!(
                                "Adoption of subscription {} for customer {customer_id} failed to save (skipping): {err}",
                                raw.subscription_id
                            );
                            continue;
                        }
                    }
                    info!(
                        "Adopted Stripe subscription {} for customer {customer_id} (user {}) — its webhook never arrived",
                        raw.subscription_id, sub.user_id
                    );
                    adopted += 1;
                }
                (None, None) => {}
            }
        }

        // Pass 2 — enforce expired grace deadlines. Queried separately and by
        // ANY status: a real cancellation leaves a terminal `canceled` row that
        // Pass 1's query skips, but its grace deadline must still fire.
        let mut executor = self.begin().await?;
        let expired = self
            .repository
            .list_subscriptions_with_expired_grace_deadline(&mut executor, now)
            .await?;
        executor.commit().await.map_err(|err| {
            UniversalInboxError::Unexpected(anyhow!(
                "Failed to commit the expired-grace listing: {err}"
            ))
        })?;
        for sub in &expired {
            if !dry_run {
                // Enforcement and the deadline clear are one unit: a deadline
                // cleared without the pause leaves the user over the cap with
                // nothing left to fire.
                if let Err(err) = self
                    .write_in_own_transaction(async |tx| {
                        self.enforce_free_plan_compliance(tx, sub.user_id).await?;
                        self.repository
                            .set_over_limit_grace_deadline(tx, sub.user_id, None)
                            .await
                    })
                    .await
                {
                    warn!(
                        "Grace expiry for user {} failed (skipping): {err}",
                        sub.user_id
                    );
                    continue;
                }
            }
            grace_expired += 1;
        }

        Ok(ReconcileReport {
            refreshed,
            adopted,
            grace_expired,
        })
    }.await;
        result.record_span_error()
    }

    /// Bring a Free user back into compliance with the integration cap:
    /// pause excess validated connections (config-level toggle off + DB marker)
    /// in a deterministic order — oldest-created kept, most recently created
    /// paused first, skipping provider kinds a pause cannot restrict.
    ///
    /// No-op for Paid users. Returns the list of paused connection ids.
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields({ attr::USER_ID } = %user_id, { attr::ERROR_TYPE } = tracing::field::Empty)
    )]
    pub async fn enforce_free_plan_compliance(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<
        Vec<universal_inbox::integration_connection::IntegrationConnectionId>,
        UniversalInboxError,
    > {
        let result: Result<
            Vec<universal_inbox::integration_connection::IntegrationConnectionId>,
            UniversalInboxError,
        > = async move {
            use crate::repository::integration_connection::IntegrationConnectionRepository;
            use universal_inbox::integration_connection::{
                IntegrationConnectionStatus, provider::IntegrationProviderKind,
            };

            let plan = self.get_user_plan(executor, user_id).await?;
            if plan.is_paid() {
                return Ok(vec![]);
            }

            let mut connections = self
                .repository
                .fetch_all_integration_connections(
                    executor,
                    user_id,
                    Some(IntegrationConnectionStatus::Validated),
                    false,
                )
                .await?;
            // Exclude the implicit `API` connection: it is auto-created, hidden from
            // the integrations panel, and excluded from the billing usage count, so
            // it must neither tip the over-limit decision nor be selected for
            // pausing. Keeps this path in lockstep with
            // `count_validated_integration_connections`.
            // Measure the excess the way the cap is measured, or the two disagree
            // and this pauses connections the user does not owe. Both exclusions
            // mirror `count_validated_integration_connections`: the implicit `API`
            // connection, and connections already paused, which consume no slot
            // and whose `auto_paused_config_snapshot` a second pass would
            // overwrite with the disabled config.
            connections.retain(|c| {
                c.provider.kind() != IntegrationProviderKind::API
                    && c.auto_paused_by_plan_at.is_none()
            });
            let limit = self.limits.max_integration_connections as usize;
            if connections.len() <= limit {
                return Ok(vec![]);
            }

            // Deterministic ordering: keep the oldest-created connections, so the
            // most recently added are the ones paused when a Free user is over the
            // cap. Iterated newest-first below, pausing until the excess is gone.
            connections.sort_by_key(|c| c.created_at);
            let excess = connections.len() - limit;

            let now = Utc::now();
            let mut paused = vec![];
            let mut unenforceable = vec![];
            for connection in connections.into_iter().rev() {
                if paused.len() == excess {
                    break;
                }
                // A provider kind with no sync toggle cannot be paused: the
                // marker alone would free its slot while it kept working. Leave it
                // counted and pause the next-newest connection instead.
                if !connection.provider.has_sync_toggles() {
                    unenforceable.push(connection.id);
                    continue;
                }
                let connection_id = connection.id;
                // Snapshot the user's real config BEFORE disabling so an upgrade
                // can restore their exact toggles, not blanket-enable everything.
                let snapshot = connection.provider.config();
                let mut updated_provider = connection.provider.clone();
                let changed = updated_provider.disable_all_syncs();
                if changed {
                    self.repository
                        .update_integration_connection_config(
                            executor,
                            connection_id,
                            updated_provider.config(),
                            user_id,
                        )
                        .await?;
                }
                self.repository
                    .set_integration_connection_plan_pause(
                        executor,
                        connection_id,
                        Some(now),
                        Some(&snapshot),
                    )
                    .await?;
                paused.push(connection_id);
            }

            if paused.len() < excess {
                // Nothing pausable is left, so the user stays over the cap and
                // keeps the banner — better than a marker that reports a
                // compliance it did not achieve.
                warn!(
                    "Free-plan enforcement for user {user_id} paused {} of {excess} excess \
                 connections; {} cannot be restricted by a pause ({:?})",
                    paused.len(),
                    unenforceable.len(),
                    unenforceable
                );
            }
            Ok(paused)
        }
        .await;
        result.record_span_error()
    }

    /// Inverse of [`Self::enforce_free_plan_compliance`]: for every connection
    /// this user has that is currently plan-paused, restore the config captured
    /// at pause time and clear both the marker and the snapshot. Invoked on the
    /// Free → Paid transition. No-op when nothing is paused.
    pub async fn restore_plan_paused_connections(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<
        Vec<universal_inbox::integration_connection::IntegrationConnectionId>,
        UniversalInboxError,
    > {
        use crate::repository::integration_connection::IntegrationConnectionRepository;

        let connections = self
            .repository
            .fetch_all_integration_connections(executor, user_id, None, false)
            .await?;

        let mut restored = vec![];
        for connection in connections {
            let (Some(_), Some(snapshot)) = (
                connection.auto_paused_by_plan_at,
                connection.auto_paused_config_snapshot.as_ref(),
            ) else {
                continue;
            };
            let connection_id = connection.id;
            self.repository
                .update_integration_connection_config(
                    executor,
                    connection_id,
                    snapshot.clone(),
                    user_id,
                )
                .await?;
            // Clear marker + snapshot together (None, None) to keep the
            // paired-NULL invariant.
            self.repository
                .set_integration_connection_plan_pause(executor, connection_id, None, None)
                .await?;
            restored.push(connection_id);
        }
        Ok(restored)
    }

    async fn ensure_stripe_customer(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user: &User,
    ) -> Result<StripeCustomerId, UniversalInboxError> {
        // Serialize concurrent customer creation for this user before the first
        // read so two checkout requests can't both mint a Stripe customer.
        self.repository
            .acquire_user_advisory_lock(executor, user.id)
            .await?;

        if let Some(existing) = self
            .repository
            .get_user_subscription(executor, user.id)
            .await?
        {
            // Reuse if we already minted one (a pending row has `None`).
            if let Some(customer_id) = existing.stripe_customer_id {
                return Ok(customer_id);
            }
        }

        let customer_id = StripeCustomerId(
            self.stripe
                .create_customer(user.id, user.email.as_ref().map(|e| e.as_ref()))
                .await?,
        );

        let existing = self
            .repository
            .get_user_subscription(executor, user.id)
            .await?;
        let now = Utc::now();
        let row = UserSubscription {
            stripe_customer_id: Some(customer_id.clone()),
            ..UserSubscription::carry_forward(user.id, existing.as_ref(), now)
        };
        self.repository
            .upsert_user_subscription(executor, &row)
            .await?;
        Ok(customer_id)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SyncKind {
    Notifications,
    Tasks,
}

/// Whether Stripe could still invoice a subscription in this status, i.e.
/// whether account deletion must cancel it. Stricter than
/// `SubscriptionStatus::is_terminal`: an `unpaid` subscription is terminal
/// for entitlements but Stripe still retries / invoices it until cancelled.
fn still_billable(status: universal_inbox::billing::SubscriptionStatus) -> bool {
    use universal_inbox::billing::SubscriptionStatus;
    !matches!(
        status,
        SubscriptionStatus::Canceled | SubscriptionStatus::IncompleteExpired
    )
}

/// Paid users see the global floor unchanged; Free users get
/// `max(global, plan_floor)`. Extracted from the service so it can be
/// unit-tested without a database handle.
pub fn effective_sync_interval(
    limits: &BillingLimits,
    plan: Plan,
    global_floor_in_minutes: i64,
    kind: SyncKind,
) -> i64 {
    match plan {
        Plan::Paid => global_floor_in_minutes,
        Plan::Free => {
            let plan_floor = match kind {
                SyncKind::Notifications => limits.notification_sync_interval_in_minutes,
                SyncKind::Tasks => limits.task_sync_interval_in_minutes,
            };
            global_floor_in_minutes.max(plan_floor)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> BillingLimits {
        BillingLimits {
            max_integration_connections: 2,
            notification_sync_interval_in_minutes: 1440,
            task_sync_interval_in_minutes: 360,
            rollout_grace_days: 30,
        }
    }

    /// Free plan: effective interval is the longer of (global, plan) for both
    /// notification and task sync.
    #[test]
    fn free_plan_takes_max_of_global_and_plan_intervals() {
        let l = limits();

        assert_eq!(
            effective_sync_interval(&l, Plan::Free, 60, SyncKind::Notifications),
            1440,
            "plan floor wins when greater"
        );
        assert_eq!(
            effective_sync_interval(&l, Plan::Free, 5000, SyncKind::Notifications),
            5000,
            "global floor wins when greater"
        );
        assert_eq!(
            effective_sync_interval(&l, Plan::Free, 60, SyncKind::Tasks),
            360
        );
        assert_eq!(
            effective_sync_interval(&l, Plan::Free, 1000, SyncKind::Tasks),
            1000
        );
    }

    /// Paid plan: plan floor never applies — only the global setting matters.
    #[test]
    fn paid_plan_ignores_plan_floor() {
        let l = limits();
        assert_eq!(
            effective_sync_interval(&l, Plan::Paid, 2, SyncKind::Notifications),
            2
        );
        assert_eq!(
            effective_sync_interval(&l, Plan::Paid, 5, SyncKind::Tasks),
            5
        );
    }
}
