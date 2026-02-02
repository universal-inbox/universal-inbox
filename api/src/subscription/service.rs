use std::sync::Arc;

use anyhow::anyhow;
use chrono::{DateTime, Duration, Utc};
use sqlx::{Postgres, Transaction};
use stripe::SubscriptionStatus as StripeSubscriptionStatus;
use url::Url;

use universal_inbox::{
    subscription::{BillingInterval, SubscriptionInfo, SubscriptionStatus, UserSubscription},
    user::UserId,
};

use crate::{
    configuration::StripeConfig,
    subscription::{
        repository::{SubscriptionRepository, SubscriptionRepositoryImpl},
        stripe::StripeService,
    },
    universal_inbox::{UniversalInboxError, UpdateStatus},
};

const TRIAL_DURATION_DAYS: i64 = 30;

pub struct SubscriptionService {
    repository: Arc<SubscriptionRepositoryImpl>,
    stripe_service: Option<StripeService>,
    stripe_enabled: bool,
}

impl SubscriptionService {
    pub fn new(
        repository: Arc<SubscriptionRepositoryImpl>,
        stripe_config: StripeConfig,
    ) -> Result<Self, UniversalInboxError> {
        let stripe_enabled = stripe_config.enabled;
        let stripe_service = StripeService::new(stripe_config)?;

        Ok(Self {
            repository,
            stripe_service,
            stripe_enabled,
        })
    }

    pub async fn begin(&self) -> Result<Transaction<'_, Postgres>, UniversalInboxError> {
        self.repository.begin().await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = %user_id),
        err
    )]
    pub async fn start_trial(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<UserSubscription, UniversalInboxError> {
        let subscription = if self.stripe_enabled {
            let trial_ends_at = Utc::now() + Duration::days(TRIAL_DURATION_DAYS);
            UserSubscription::new_trial(user_id, trial_ends_at)
        } else {
            UserSubscription::unlimited(user_id)
        };

        self.repository
            .create_subscription(executor, subscription)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = %user_id),
        err
    )]
    pub async fn get_subscription_status(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<SubscriptionInfo, UniversalInboxError> {
        if !self.stripe_enabled {
            return Ok(SubscriptionInfo::unlimited());
        }

        match self
            .repository
            .get_subscription_by_user_id(executor, user_id)
            .await?
        {
            Some(subscription) => {
                let subscription = self
                    .check_and_update_trial_expiry(executor, subscription)
                    .await?;
                Ok(SubscriptionInfo::from_subscription(&subscription))
            }
            None => Ok(SubscriptionInfo::unlimited()),
        }
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = %user_id),
        err
    )]
    pub async fn is_feature_access_allowed(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<bool, UniversalInboxError> {
        self.get_subscription_status(executor, user_id)
            .await
            .map(|info| !info.is_active())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = %user_id),
        err
    )]
    pub async fn is_read_only_mode(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<bool, UniversalInboxError> {
        self.get_subscription_status(executor, user_id)
            .await
            .map(|info| info.is_read_only())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = %user_id),
        err
    )]
    pub async fn sync_subscription_from_stripe(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<UpdateStatus<UserSubscription>, UniversalInboxError> {
        let Some(stripe_service) = &self.stripe_service else {
            return Ok(UpdateStatus {
                updated: false,
                result: None,
            });
        };

        let Some(subscription) = self
            .repository
            .get_subscription_by_user_id(executor, user_id)
            .await?
        else {
            return Ok(UpdateStatus {
                updated: false,
                result: None,
            });
        };

        let Some(stripe_subscription_id) = &subscription.subscription_id else {
            return Ok(UpdateStatus {
                updated: false,
                result: Some(subscription),
            });
        };

        let stripe_subscription = stripe_service
            .get_subscription(stripe_subscription_id)
            .await?;

        let updated_subscription = self.apply_stripe_subscription_update(
            subscription,
            &stripe_subscription.status,
            stripe_subscription.current_period_end,
            stripe_subscription.billing_interval,
        );

        self.repository
            .update_subscription(executor, &updated_subscription)
            .await
    }

    async fn check_and_update_trial_expiry(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        mut subscription: UserSubscription,
    ) -> Result<UserSubscription, UniversalInboxError> {
        if subscription.subscription_status == SubscriptionStatus::Trialing
            && let Some(trial_ends_at) = subscription.trial_ends_at
            && Utc::now() > trial_ends_at
        {
            subscription.subscription_status = SubscriptionStatus::Expired;
            let update_result = self
                .repository
                .update_subscription(executor, &subscription)
                .await?;
            if let Some(updated) = update_result.result {
                return Ok(updated);
            }
        }
        Ok(subscription)
    }

    fn apply_stripe_subscription_update(
        &self,
        mut subscription: UserSubscription,
        stripe_status: &StripeSubscriptionStatus,
        current_period_end: Option<DateTime<Utc>>,
        billing_interval: Option<BillingInterval>,
    ) -> UserSubscription {
        subscription.subscription_status = match stripe_status {
            StripeSubscriptionStatus::Active => SubscriptionStatus::Active,
            StripeSubscriptionStatus::Trialing => SubscriptionStatus::Trialing,
            StripeSubscriptionStatus::PastDue => SubscriptionStatus::PastDue,
            StripeSubscriptionStatus::Canceled => SubscriptionStatus::Canceled,
            StripeSubscriptionStatus::Incomplete
            | StripeSubscriptionStatus::IncompleteExpired
            | StripeSubscriptionStatus::Unpaid => SubscriptionStatus::Expired,
            StripeSubscriptionStatus::Paused => SubscriptionStatus::Canceled,
        };

        subscription.subscription_ends_at = current_period_end;
        subscription.billing_interval = billing_interval;
        subscription.updated_at = Utc::now();

        subscription
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = %user_id),
        err
    )]
    pub async fn get_subscription_by_user_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Option<UserSubscription>, UniversalInboxError> {
        self.repository
            .get_subscription_by_user_id(executor, user_id)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(subscription.id = %subscription.id),
        err
    )]
    pub async fn update_subscription(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        subscription: &UserSubscription,
    ) -> Result<UpdateStatus<UserSubscription>, UniversalInboxError> {
        self.repository
            .update_subscription(executor, subscription)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = %user_id, billing_interval = ?billing_interval),
        err
    )]
    pub async fn create_checkout_session(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        billing_interval: BillingInterval,
        success_url: &str,
        cancel_url: &str,
    ) -> Result<Url, UniversalInboxError> {
        let stripe_service = self.stripe_service.as_ref().ok_or_else(|| {
            UniversalInboxError::UnsupportedAction("Stripe billing is not enabled".to_string())
        })?;

        let subscription = self
            .repository
            .get_subscription_by_user_id(executor, user_id)
            .await?
            .ok_or_else(|| {
                UniversalInboxError::ItemNotFound(format!(
                    "No subscription found for user {user_id}. User must be registered first."
                ))
            })?;

        let price_id = match billing_interval {
            BillingInterval::Month => stripe_service.price_id_monthly().ok_or_else(|| {
                UniversalInboxError::Unexpected(anyhow!("Monthly price ID is not configured"))
            })?,
            BillingInterval::Year => stripe_service.price_id_annual().ok_or_else(|| {
                UniversalInboxError::Unexpected(anyhow!("Annual price ID is not configured"))
            })?,
        };

        let customer_id = subscription.stripe_customer_id.ok_or_else(|| {
            UniversalInboxError::InvalidInputData {
                source: None,
                user_error: "Stripe customer not yet created. Please contact support.".to_string(),
            }
        })?;

        stripe_service
            .create_checkout_session(&customer_id, price_id, success_url, cancel_url)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = %user_id),
        err
    )]
    pub async fn create_portal_session(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        return_url: &str,
    ) -> Result<Url, UniversalInboxError> {
        let stripe_service = self.stripe_service.as_ref().ok_or_else(|| {
            UniversalInboxError::UnsupportedAction("Stripe billing is not enabled".to_string())
        })?;

        let subscription = self
            .repository
            .get_subscription_by_user_id(executor, user_id)
            .await?
            .ok_or_else(|| {
                UniversalInboxError::ItemNotFound(format!(
                    "No subscription found for user {user_id}"
                ))
            })?;

        let customer_id = subscription.stripe_customer_id.ok_or_else(|| {
            UniversalInboxError::InvalidInputData {
                source: None,
                user_error: "No Stripe customer associated with this subscription".to_string(),
            }
        })?;

        stripe_service
            .create_portal_session(&customer_id, return_url)
            .await
    }

    pub fn stripe_service(&self) -> Option<&StripeService> {
        self.stripe_service.as_ref()
    }

    pub fn is_stripe_enabled(&self) -> bool {
        self.stripe_enabled
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(stripe_customer_id = %stripe_customer_id),
        err
    )]
    pub async fn get_subscription_by_stripe_customer_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        stripe_customer_id: &str,
    ) -> Result<Option<UserSubscription>, UniversalInboxError> {
        self.repository
            .get_subscription_by_stripe_customer_id(executor, stripe_customer_id)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            stripe_customer_id = %stripe_customer_id,
            subscription_id = %subscription_id
        ),
        err
    )]
    pub async fn handle_checkout_session_completed(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        stripe_customer_id: &str,
        subscription_id: &str,
        stripe_status: &StripeSubscriptionStatus,
        current_period_end: Option<DateTime<Utc>>,
        billing_interval: Option<BillingInterval>,
    ) -> Result<UpdateStatus<UserSubscription>, UniversalInboxError> {
        let Some(mut subscription) = self
            .repository
            .get_subscription_by_stripe_customer_id(executor, stripe_customer_id)
            .await?
        else {
            tracing::warn!(
                "No subscription found for Stripe customer {stripe_customer_id} during checkout completion"
            );
            return Ok(UpdateStatus {
                updated: false,
                result: None,
            });
        };

        subscription.subscription_id = Some(subscription_id.to_string());
        let updated_subscription = self.apply_stripe_subscription_update(
            subscription,
            stripe_status,
            current_period_end,
            billing_interval,
        );

        self.repository
            .update_subscription(executor, &updated_subscription)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(subscription_id = %subscription_id),
        err
    )]
    pub async fn handle_subscription_updated(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        subscription_id: &str,
        stripe_status: &StripeSubscriptionStatus,
        current_period_end: Option<DateTime<Utc>>,
        billing_interval: Option<BillingInterval>,
    ) -> Result<UpdateStatus<UserSubscription>, UniversalInboxError> {
        let Some(subscription) = self
            .repository
            .get_subscription_by_subscription_id(executor, subscription_id)
            .await?
        else {
            tracing::warn!(
                "No subscription found for Stripe subscription {subscription_id} during update"
            );
            return Ok(UpdateStatus {
                updated: false,
                result: None,
            });
        };

        let updated_subscription = self.apply_stripe_subscription_update(
            subscription,
            stripe_status,
            current_period_end,
            billing_interval,
        );

        self.repository
            .update_subscription(executor, &updated_subscription)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(subscription_id = %subscription_id),
        err
    )]
    pub async fn handle_subscription_deleted(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        subscription_id: &str,
    ) -> Result<UpdateStatus<UserSubscription>, UniversalInboxError> {
        let Some(mut subscription) = self
            .repository
            .get_subscription_by_subscription_id(executor, subscription_id)
            .await?
        else {
            tracing::warn!(
                "No subscription found for Stripe subscription {subscription_id} during deletion"
            );
            return Ok(UpdateStatus {
                updated: false,
                result: None,
            });
        };

        subscription.subscription_status = SubscriptionStatus::Canceled;
        subscription.updated_at = Utc::now();

        self.repository
            .update_subscription(executor, &subscription)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(subscription_id = %subscription_id),
        err
    )]
    pub async fn handle_invoice_payment_failed(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        subscription_id: &str,
    ) -> Result<UpdateStatus<UserSubscription>, UniversalInboxError> {
        let Some(mut subscription) = self
            .repository
            .get_subscription_by_subscription_id(executor, subscription_id)
            .await?
        else {
            tracing::warn!(
                "No subscription found for Stripe subscription {subscription_id} during payment failure"
            );
            return Ok(UpdateStatus {
                updated: false,
                result: None,
            });
        };

        subscription.subscription_status = SubscriptionStatus::PastDue;
        subscription.updated_at = Utc::now();

        self.repository
            .update_subscription(executor, &subscription)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(subscription_id = %subscription_id),
        err
    )]
    pub async fn handle_invoice_paid(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        subscription_id: &str,
    ) -> Result<UpdateStatus<UserSubscription>, UniversalInboxError> {
        let Some(mut subscription) = self
            .repository
            .get_subscription_by_subscription_id(executor, subscription_id)
            .await?
        else {
            tracing::warn!(
                "No subscription found for Stripe subscription {subscription_id} during payment success"
            );
            return Ok(UpdateStatus {
                updated: false,
                result: None,
            });
        };

        if subscription.subscription_status == SubscriptionStatus::PastDue {
            subscription.subscription_status = SubscriptionStatus::Active;
            subscription.updated_at = Utc::now();

            self.repository
                .update_subscription(executor, &subscription)
                .await
        } else {
            Ok(UpdateStatus {
                updated: false,
                result: Some(subscription),
            })
        }
    }

    /// Sync all subscriptions that have a Stripe subscription ID from Stripe.
    /// This is used as a fallback mechanism to handle missed webhooks.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub async fn sync_all_subscriptions_from_stripe(&self) -> Result<usize, UniversalInboxError> {
        let Some(stripe_service) = &self.stripe_service else {
            tracing::debug!("Stripe not enabled, skipping subscription sync");
            return Ok(0);
        };

        let mut transaction = self.begin().await?;

        let subscriptions = self
            .repository
            .list_subscriptions_with_stripe_subscription(&mut transaction)
            .await?;

        let mut synced_count = 0;

        for subscription in subscriptions {
            let Some(stripe_subscription_id) = subscription.subscription_id.clone() else {
                continue;
            };

            match stripe_service
                .get_subscription(&stripe_subscription_id)
                .await
            {
                Ok(stripe_subscription) => {
                    let updated_subscription = self.apply_stripe_subscription_update(
                        subscription,
                        &stripe_subscription.status,
                        stripe_subscription.current_period_end,
                        stripe_subscription.billing_interval,
                    );

                    match self
                        .repository
                        .update_subscription(&mut transaction, &updated_subscription)
                        .await
                    {
                        Ok(update_status) => {
                            if update_status.updated {
                                synced_count += 1;
                                tracing::info!(
                                    subscription_id = %stripe_subscription_id,
                                    user_id = %updated_subscription.user_id,
                                    "Synced subscription from Stripe"
                                );
                            }
                        }
                        Err(e) => {
                            tracing::error!(
                                subscription_id = %stripe_subscription_id,
                                error = %e,
                                "Failed to update subscription from Stripe sync"
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(
                        subscription_id = %stripe_subscription_id,
                        error = %e,
                        "Failed to fetch subscription from Stripe during sync"
                    );
                }
            }
        }

        transaction
            .commit()
            .await
            .map_err(|e| UniversalInboxError::DatabaseError {
                source: e,
                message: "Failed to commit subscription sync transaction".to_string(),
            })?;

        tracing::info!(synced_count, "Completed subscription sync from Stripe");
        Ok(synced_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use uuid::Uuid;

    #[test]
    fn test_subscription_info_from_trial() {
        let user_id: UserId = Uuid::new_v4().into();
        let trial_ends_at = Utc::now() + Duration::days(15);
        let subscription = UserSubscription::new_trial(user_id, trial_ends_at);

        let info = SubscriptionInfo::from_subscription(&subscription);

        assert_eq!(info.status, SubscriptionStatus::Trialing);
        assert!(info.trial_ends_at.is_some());
        assert!(info.days_remaining.is_some());
        assert!(info.days_remaining.unwrap() >= 14 && info.days_remaining.unwrap() <= 15);
        assert!(!info.is_read_only);
    }

    #[test]
    fn test_subscription_info_unlimited() {
        let info = SubscriptionInfo::unlimited();

        assert_eq!(info.status, SubscriptionStatus::Unlimited);
        assert!(info.trial_ends_at.is_none());
        assert!(info.subscription_ends_at.is_none());
        assert!(info.days_remaining.is_none());
        assert!(!info.is_read_only);
    }

    #[test]
    fn test_subscription_info_expired_trial() {
        let user_id: UserId = Uuid::new_v4().into();
        let trial_ends_at = Utc::now() - Duration::days(1);
        let mut subscription = UserSubscription::new_trial(user_id, trial_ends_at);
        subscription.subscription_status = SubscriptionStatus::Expired;

        let info = SubscriptionInfo::from_subscription(&subscription);

        assert_eq!(info.status, SubscriptionStatus::Expired);
        assert!(info.is_read_only);
        assert!(info.days_remaining.is_none());
    }

    #[test]
    fn test_days_remaining_zero_when_expired() {
        let user_id: UserId = Uuid::new_v4().into();
        let trial_ends_at = Utc::now() - Duration::days(5);
        let subscription = UserSubscription::new_trial(user_id, trial_ends_at);

        let info = SubscriptionInfo::from_subscription(&subscription);

        assert_eq!(info.days_remaining, Some(0));
    }
}
