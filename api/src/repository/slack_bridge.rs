use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, Postgres, Row, Transaction};
use uuid::Uuid;

use universal_inbox::{
    slack_bridge::{
        SlackBridgeActionStatus, SlackBridgeActionType, SlackBridgePendingAction,
        SlackBridgePendingActionId,
    },
    user::UserId,
};

use crate::{repository::Repository, universal_inbox::UniversalInboxError};

const MAX_RETRIES: i32 = 5;
const BACKOFF_BASE_DELAY_SECONDS: f64 = 30.0;
const BACKOFF_MAX_DELAY_SECONDS: f64 = 600.0;

#[derive(Debug, FromRow)]
struct SlackBridgePendingActionRow {
    id: Uuid,
    user_id: Uuid,
    notification_id: Option<Uuid>,
    action_type: String,
    slack_team_id: String,
    slack_channel_id: String,
    slack_thread_ts: String,
    slack_last_message_ts: String,
    status: String,
    failure_message: Option<String>,
    retry_count: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
}

impl TryFrom<SlackBridgePendingActionRow> for SlackBridgePendingAction {
    type Error = UniversalInboxError;

    fn try_from(row: SlackBridgePendingActionRow) -> Result<Self, Self::Error> {
        Ok(SlackBridgePendingAction {
            id: row.id.into(),
            user_id: row.user_id.into(),
            notification_id: row.notification_id.map(|id| id.into()),
            action_type: match row.action_type.as_str() {
                "MarkAsRead" => SlackBridgeActionType::MarkAsRead,
                "Unsubscribe" => SlackBridgeActionType::Unsubscribe,
                other => {
                    return Err(UniversalInboxError::Unexpected(anyhow::anyhow!(
                        "Unknown SlackBridgeActionType: {other}"
                    )));
                }
            },
            slack_team_id: row.slack_team_id.into(),
            slack_channel_id: row.slack_channel_id.into(),
            slack_thread_ts: row.slack_thread_ts.into(),
            slack_last_message_ts: row.slack_last_message_ts.into(),
            status: match row.status.as_str() {
                "Pending" => SlackBridgeActionStatus::Pending,
                "Completed" => SlackBridgeActionStatus::Completed,
                "Failed" => SlackBridgeActionStatus::Failed,
                "PermanentlyFailed" => SlackBridgeActionStatus::PermanentlyFailed,
                other => {
                    return Err(UniversalInboxError::Unexpected(anyhow::anyhow!(
                        "Unknown SlackBridgeActionStatus: {other}"
                    )));
                }
            },
            failure_message: row.failure_message,
            retry_count: row.retry_count,
            created_at: row.created_at,
            updated_at: row.updated_at,
            completed_at: row.completed_at,
        })
    }
}

#[async_trait]
pub trait SlackBridgeRepository {
    async fn create_pending_action(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        action: &SlackBridgePendingAction,
    ) -> Result<SlackBridgePendingAction, UniversalInboxError>;

    async fn get_actionable_actions(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Vec<SlackBridgePendingAction>, UniversalInboxError>;

    async fn mark_action_completed(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        action_id: SlackBridgePendingActionId,
        user_id: UserId,
    ) -> Result<Option<SlackBridgePendingAction>, UniversalInboxError>;

    async fn mark_action_failed(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        action_id: SlackBridgePendingActionId,
        user_id: UserId,
        error: &str,
    ) -> Result<Option<SlackBridgePendingAction>, UniversalInboxError>;

    async fn get_bridge_status(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<(i64, i64, Option<DateTime<Utc>>), UniversalInboxError>;
}

#[async_trait]
impl SlackBridgeRepository for Repository {
    #[tracing::instrument(level = "debug", skip_all, fields(action_id = action.id.to_string()), err)]
    async fn create_pending_action(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        action: &SlackBridgePendingAction,
    ) -> Result<SlackBridgePendingAction, UniversalInboxError> {
        let row: SlackBridgePendingActionRow = sqlx::query_as(
            r#"
                INSERT INTO slack_bridge_pending_action
                    (id, user_id, notification_id, action_type, slack_team_id,
                     slack_channel_id, slack_thread_ts, slack_last_message_ts,
                     status, created_at, updated_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                RETURNING *
            "#,
        )
        .bind(action.id.0)
        .bind(action.user_id.0)
        .bind(action.notification_id.map(|id| id.0))
        .bind(action.action_type.to_string())
        .bind(action.slack_team_id.to_string())
        .bind(action.slack_channel_id.to_string())
        .bind(action.slack_thread_ts.to_string())
        .bind(action.slack_last_message_ts.to_string())
        .bind(action.status.to_string())
        .bind(action.created_at)
        .bind(action.updated_at)
        .fetch_one(&mut **executor)
        .await
        .map_err(|err| UniversalInboxError::DatabaseError {
            source: err,
            message: "Failed to create slack bridge pending action".to_string(),
        })?;

        row.try_into()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(user_id = user_id.to_string()), err)]
    async fn get_actionable_actions(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Vec<SlackBridgePendingAction>, UniversalInboxError> {
        let rows: Vec<SlackBridgePendingActionRow> = sqlx::query_as(
            r#"
                SELECT *
                FROM slack_bridge_pending_action
                WHERE user_id = $1
                  AND (
                    (status = 'Pending' AND retry_count = 0)
                    OR (status = 'Failed' AND updated_at + make_interval(secs =>
                        LEAST(
                            $2 * POWER(2, GREATEST(retry_count - 1, 0)),
                            $3
                        )
                    ) < NOW())
                  )
                ORDER BY created_at ASC
            "#,
        )
        .bind(user_id.0)
        .bind(BACKOFF_BASE_DELAY_SECONDS)
        .bind(BACKOFF_MAX_DELAY_SECONDS)
        .fetch_all(&mut **executor)
        .await
        .map_err(|err| UniversalInboxError::DatabaseError {
            source: err,
            message: "Failed to fetch pending slack bridge actions".to_string(),
        })?;

        rows.into_iter().map(|row| row.try_into()).collect()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(action_id = action_id.to_string(), user_id = user_id.to_string()), err)]
    async fn mark_action_completed(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        action_id: SlackBridgePendingActionId,
        user_id: UserId,
    ) -> Result<Option<SlackBridgePendingAction>, UniversalInboxError> {
        let row: Option<SlackBridgePendingActionRow> = sqlx::query_as(
            r#"
                UPDATE slack_bridge_pending_action
                SET status = 'Completed',
                    completed_at = NOW(),
                    updated_at = NOW()
                WHERE id = $1 AND user_id = $2
                RETURNING *
            "#,
        )
        .bind(action_id.0)
        .bind(user_id.0)
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| UniversalInboxError::DatabaseError {
            source: err,
            message: "Failed to mark slack bridge action as completed".to_string(),
        })?;

        row.map(|r| r.try_into()).transpose()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(action_id = action_id.to_string(), user_id = user_id.to_string()), err)]
    async fn mark_action_failed(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        action_id: SlackBridgePendingActionId,
        user_id: UserId,
        error: &str,
    ) -> Result<Option<SlackBridgePendingAction>, UniversalInboxError> {
        let row: Option<SlackBridgePendingActionRow> = sqlx::query_as(
            r#"
                UPDATE slack_bridge_pending_action
                SET status = CASE
                        WHEN retry_count + 1 >= $2 THEN 'PermanentlyFailed'
                        ELSE 'Failed'
                    END,
                    failure_message = $3,
                    retry_count = retry_count + 1,
                    updated_at = NOW()
                WHERE id = $1 AND user_id = $4
                RETURNING *
            "#,
        )
        .bind(action_id.0)
        .bind(MAX_RETRIES)
        .bind(error)
        .bind(user_id.0)
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| UniversalInboxError::DatabaseError {
            source: err,
            message: "Failed to mark slack bridge action as failed".to_string(),
        })?;

        row.map(|r| r.try_into()).transpose()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(user_id = user_id.to_string()), err)]
    async fn get_bridge_status(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<(i64, i64, Option<DateTime<Utc>>), UniversalInboxError> {
        let row = sqlx::query(
            r#"
                SELECT
                    COALESCE(SUM(CASE WHEN status = 'Pending' THEN 1 ELSE 0 END), 0) as pending_count,
                    COALESCE(SUM(CASE WHEN status = 'Failed' THEN 1 ELSE 0 END), 0) as failed_count,
                    MAX(CASE WHEN status = 'Completed' THEN completed_at END) as last_completed_at
                FROM slack_bridge_pending_action
                WHERE user_id = $1
            "#,
        )
        .bind(user_id.0)
        .fetch_one(&mut **executor)
        .await
        .map_err(|err| UniversalInboxError::DatabaseError {
            source: err,
            message: "Failed to get slack bridge status".to_string(),
        })?;

        let pending_count: i64 = row.get("pending_count");
        let failed_count: i64 = row.get("failed_count");
        let last_completed_at: Option<DateTime<Utc>> = row.get("last_completed_at");

        Ok((pending_count, failed_count, last_completed_at))
    }
}
