use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use universal_inbox::{
    subscription::{BillingInterval, SubscriptionId, SubscriptionStatus, UserSubscription},
    user::UserId,
};

use crate::universal_inbox::{UniversalInboxError, UpdateStatus};

#[async_trait]
pub trait SubscriptionRepository {
    async fn get_subscription(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: SubscriptionId,
    ) -> Result<Option<UserSubscription>, UniversalInboxError>;

    async fn get_subscription_by_user_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Option<UserSubscription>, UniversalInboxError>;

    async fn create_subscription(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        subscription: UserSubscription,
    ) -> Result<UserSubscription, UniversalInboxError>;

    async fn update_subscription(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        subscription: &UserSubscription,
    ) -> Result<UpdateStatus<UserSubscription>, UniversalInboxError>;

    async fn delete_subscription(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: SubscriptionId,
    ) -> Result<bool, UniversalInboxError>;
}

#[derive(Debug)]
pub struct SubscriptionRepositoryImpl {
    pub pool: Arc<PgPool>,
}

impl SubscriptionRepositoryImpl {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    pub async fn begin(&self) -> Result<Transaction<'_, Postgres>, UniversalInboxError> {
        self.pool
            .begin()
            .await
            .map_err(|err| UniversalInboxError::DatabaseError {
                source: err,
                message: "Failed to begin database transaction".to_string(),
            })
    }
}

#[async_trait]
impl SubscriptionRepository for SubscriptionRepositoryImpl {
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(subscription.id = id.to_string()),
        err
    )]
    async fn get_subscription(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: SubscriptionId,
    ) -> Result<Option<UserSubscription>, UniversalInboxError> {
        let row = sqlx::query_as!(
            UserSubscriptionRow,
            r#"
                SELECT
                    id,
                    user_id,
                    stripe_customer_id,
                    subscription_status as "subscription_status: _",
                    subscription_id,
                    trial_started_at,
                    trial_ends_at,
                    subscription_ends_at,
                    billing_interval,
                    created_at,
                    updated_at
                FROM user_subscription
                WHERE id = $1
            "#,
            id.0
        )
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!("Failed to fetch subscription {id} from storage: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        row.map(|r| r.try_into()).transpose()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = user_id.to_string()),
        err
    )]
    async fn get_subscription_by_user_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Option<UserSubscription>, UniversalInboxError> {
        let row = sqlx::query_as!(
            UserSubscriptionRow,
            r#"
                SELECT
                    id,
                    user_id,
                    stripe_customer_id,
                    subscription_status as "subscription_status: _",
                    subscription_id,
                    trial_started_at,
                    trial_ends_at,
                    subscription_ends_at,
                    billing_interval,
                    created_at,
                    updated_at
                FROM user_subscription
                WHERE user_id = $1
            "#,
            user_id.0
        )
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| {
            let message =
                format!("Failed to fetch subscription for user {user_id} from storage: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        row.map(|r| r.try_into()).transpose()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            subscription.id = subscription.id.to_string(),
            subscription.user_id = subscription.user_id.to_string()
        ),
        err
    )]
    async fn create_subscription(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        subscription: UserSubscription,
    ) -> Result<UserSubscription, UniversalInboxError> {
        let subscription_id: Uuid = sqlx::query_scalar!(
            r#"
                INSERT INTO user_subscription
                    (
                        id,
                        user_id,
                        stripe_customer_id,
                        subscription_status,
                        subscription_id,
                        trial_started_at,
                        trial_ends_at,
                        subscription_ends_at,
                        billing_interval,
                        created_at,
                        updated_at
                    )
                VALUES ($1, $2, $3, $4::subscription_status, $5, $6, $7, $8, $9, $10, $11)
                RETURNING id
            "#,
            subscription.id.0,
            subscription.user_id.0,
            subscription.stripe_customer_id,
            subscription.subscription_status.to_string() as _,
            subscription.subscription_id,
            subscription.trial_started_at,
            subscription.trial_ends_at,
            subscription.subscription_ends_at,
            subscription.billing_interval.map(|bi| bi.to_string()),
            subscription.created_at,
            subscription.updated_at
        )
        .fetch_one(&mut **executor)
        .await
        .map_err(|e| {
            match e
                .as_database_error()
                .and_then(|db_error| db_error.code().map(|code| code.to_string()))
            {
                Some(x) if x == *"23505" => UniversalInboxError::AlreadyExists {
                    source: Some(e),
                    id: subscription.id.0,
                },
                _ => UniversalInboxError::Unexpected(anyhow!(
                    "Failed to create subscription for user {}: {e}",
                    subscription.user_id
                )),
            }
        })?;

        Ok(UserSubscription {
            id: subscription_id.into(),
            ..subscription
        })
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            subscription.id = subscription.id.to_string(),
            subscription.user_id = subscription.user_id.to_string()
        ),
        err
    )]
    async fn update_subscription(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        subscription: &UserSubscription,
    ) -> Result<UpdateStatus<UserSubscription>, UniversalInboxError> {
        let now = Utc::now();
        let result = sqlx::query_as!(
            UpdatedUserSubscriptionRow,
            r#"
                UPDATE user_subscription
                SET
                    stripe_customer_id = $2,
                    subscription_status = $3::subscription_status,
                    subscription_id = $4,
                    trial_started_at = $5,
                    trial_ends_at = $6,
                    subscription_ends_at = $7,
                    billing_interval = $8,
                    updated_at = $9
                WHERE id = $1
                RETURNING
                    id,
                    user_id,
                    stripe_customer_id,
                    subscription_status as "subscription_status: _",
                    subscription_id,
                    trial_started_at,
                    trial_ends_at,
                    subscription_ends_at,
                    billing_interval,
                    created_at,
                    updated_at,
                    (SELECT
                        stripe_customer_id IS DISTINCT FROM $2
                        OR subscription_status::TEXT IS DISTINCT FROM $3
                        OR subscription_id IS DISTINCT FROM $4
                        OR trial_started_at IS DISTINCT FROM $5
                        OR trial_ends_at IS DISTINCT FROM $6
                        OR subscription_ends_at IS DISTINCT FROM $7
                        OR billing_interval IS DISTINCT FROM $8
                    FROM user_subscription WHERE id = $1) as "is_updated!"
            "#,
            subscription.id.0,
            subscription.stripe_customer_id,
            subscription.subscription_status.to_string() as _,
            subscription.subscription_id,
            subscription.trial_started_at,
            subscription.trial_ends_at,
            subscription.subscription_ends_at,
            subscription.billing_interval.map(|bi| bi.to_string()),
            now
        )
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!(
                "Failed to update subscription {} from storage: {err}",
                subscription.id
            );
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        match result {
            Some(row) => Ok(UpdateStatus {
                updated: row.is_updated,
                result: Some(row.try_into()?),
            }),
            None => Ok(UpdateStatus {
                updated: false,
                result: None,
            }),
        }
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(subscription.id = id.to_string()),
        err
    )]
    async fn delete_subscription(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: SubscriptionId,
    ) -> Result<bool, UniversalInboxError> {
        let result = sqlx::query!("DELETE FROM user_subscription WHERE id = $1", id.0)
            .execute(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to delete subscription {id} from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        Ok(result.rows_affected() == 1)
    }
}

#[derive(sqlx::Type, Debug)]
#[sqlx(type_name = "subscription_status")]
enum PgSubscriptionStatus {
    Trialing,
    Active,
    PastDue,
    Canceled,
    Expired,
    Unlimited,
}

impl From<PgSubscriptionStatus> for SubscriptionStatus {
    fn from(status: PgSubscriptionStatus) -> Self {
        match status {
            PgSubscriptionStatus::Trialing => SubscriptionStatus::Trialing,
            PgSubscriptionStatus::Active => SubscriptionStatus::Active,
            PgSubscriptionStatus::PastDue => SubscriptionStatus::PastDue,
            PgSubscriptionStatus::Canceled => SubscriptionStatus::Canceled,
            PgSubscriptionStatus::Expired => SubscriptionStatus::Expired,
            PgSubscriptionStatus::Unlimited => SubscriptionStatus::Unlimited,
        }
    }
}

#[derive(Debug)]
struct UpdatedUserSubscriptionRow {
    id: Uuid,
    user_id: Uuid,
    stripe_customer_id: Option<String>,
    subscription_status: PgSubscriptionStatus,
    subscription_id: Option<String>,
    trial_started_at: Option<DateTime<Utc>>,
    trial_ends_at: Option<DateTime<Utc>>,
    subscription_ends_at: Option<DateTime<Utc>>,
    billing_interval: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    is_updated: bool,
}

impl TryFrom<UpdatedUserSubscriptionRow> for UserSubscription {
    type Error = UniversalInboxError;

    fn try_from(row: UpdatedUserSubscriptionRow) -> Result<Self, Self::Error> {
        UserSubscriptionRow {
            id: row.id,
            user_id: row.user_id,
            stripe_customer_id: row.stripe_customer_id,
            subscription_status: row.subscription_status,
            subscription_id: row.subscription_id,
            trial_started_at: row.trial_started_at,
            trial_ends_at: row.trial_ends_at,
            subscription_ends_at: row.subscription_ends_at,
            billing_interval: row.billing_interval,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
        .try_into()
    }
}

#[derive(Debug, sqlx::FromRow)]
struct UserSubscriptionRow {
    id: Uuid,
    user_id: Uuid,
    stripe_customer_id: Option<String>,
    subscription_status: PgSubscriptionStatus,
    subscription_id: Option<String>,
    trial_started_at: Option<DateTime<Utc>>,
    trial_ends_at: Option<DateTime<Utc>>,
    subscription_ends_at: Option<DateTime<Utc>>,
    billing_interval: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<UserSubscriptionRow> for UserSubscription {
    type Error = UniversalInboxError;

    fn try_from(row: UserSubscriptionRow) -> Result<Self, Self::Error> {
        let billing_interval = row
            .billing_interval
            .map(|bi| {
                bi.parse::<BillingInterval>().map_err(|_| {
                    UniversalInboxError::Unexpected(anyhow!("Invalid billing interval: {bi}"))
                })
            })
            .transpose()?;

        Ok(UserSubscription {
            id: row.id.into(),
            user_id: row.user_id.into(),
            stripe_customer_id: row.stripe_customer_id,
            subscription_status: row.subscription_status.into(),
            subscription_id: row.subscription_id,
            trial_started_at: row.trial_started_at,
            trial_ends_at: row.trial_ends_at,
            subscription_ends_at: row.subscription_ends_at,
            billing_interval,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}
