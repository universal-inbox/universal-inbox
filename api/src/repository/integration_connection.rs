use anyhow::anyhow;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::{Postgres, QueryBuilder, Transaction, types::Json};
use tracing::debug;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionId, IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        provider::{IntegrationConnectionContext, IntegrationProvider, IntegrationProviderKind},
    },
    user::UserId,
};

use crate::{
    repository::Repository,
    universal_inbox::{UniversalInboxError, UpdateStatus},
};

#[derive(Debug)]
pub enum IntegrationConnectionSyncStatusUpdate {
    NotificationsSyncScheduled,
    NotificationsSyncStarted,
    NotificationsSyncCompleted,
    NotificationsSyncFailed(String),
    TasksSyncScheduled,
    TasksSyncStarted,
    TasksSyncCompleted,
    TasksSyncFailed(String),
}

#[derive(Debug)]
pub enum IntegrationConnectionSyncedBeforeFilter {
    Notifications(DateTime<Utc>),
    Tasks(DateTime<Utc>),
}

#[async_trait]
pub trait IntegrationConnectionRepository {
    async fn get_integration_connection(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
    ) -> Result<Option<IntegrationConnection>, UniversalInboxError>;

    async fn get_integration_connection_per_provider(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        for_user_id: UserId,
        integration_provider_kind: IntegrationProviderKind,
        synced_before_filter: Option<IntegrationConnectionSyncedBeforeFilter>,
        with_status: Option<IntegrationConnectionStatus>,
    ) -> Result<Option<IntegrationConnection>, UniversalInboxError>;

    async fn get_integration_connection_per_provider_user_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_provider_kind: IntegrationProviderKind,
        provider_user_id: String,
    ) -> Result<Option<IntegrationConnection>, UniversalInboxError>;

    async fn find_integration_connection_per_provider_user_ids(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_provider_kind: IntegrationProviderKind,
        provider_user_ids: Vec<String>,
    ) -> Result<Vec<IntegrationConnection>, UniversalInboxError>;

    async fn get_integration_connection_per_context(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        context: IntegrationConnectionContext,
        required_oauth_scopes: &[String],
    ) -> Result<Option<IntegrationConnection>, UniversalInboxError>;

    async fn update_integration_connection_status(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        new_status: IntegrationConnectionStatus,
        failure_message: Option<String>,
        registered_oauth_scopes: Option<Vec<String>>,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError>;

    async fn update_integration_connection_sync_status(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: Option<UserId>,
        integration_provider_kind: Option<IntegrationProviderKind>,
        sync_update: IntegrationConnectionSyncStatusUpdate,
        sync_failure_window_in_hours: i64,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError>;

    async fn fetch_all_integration_connections(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        for_user_id: UserId,
        status: Option<IntegrationConnectionStatus>,
        lock_rows: bool,
    ) -> Result<Vec<IntegrationConnection>, UniversalInboxError>;

    async fn create_integration_connection(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection: Box<IntegrationConnection>,
    ) -> Result<Box<IntegrationConnection>, UniversalInboxError>;

    async fn update_integration_connection_context(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        context: Option<IntegrationConnectionContext>,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError>;

    async fn does_integration_connection_exist(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: IntegrationConnectionId,
    ) -> Result<bool, UniversalInboxError>;

    async fn update_integration_connection_config(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        config: IntegrationConnectionConfig,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<IntegrationConnectionConfig>>, UniversalInboxError>;

    async fn update_integration_connection_provider_user_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        provider_user_id: Option<String>,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError>;
}

pub const TOO_MANY_SYNC_FAILURES_ERROR_MESSAGE: &str = "♻️ Synchronization has been failing for too long. Please try to reconnect the integration. If the issue keeps happening, please contact our support.";

#[async_trait]
impl IntegrationConnectionRepository for Repository {
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(integration_connection_id = integration_connection_id.to_string().to_string()),
        err
    )]
    async fn get_integration_connection(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
    ) -> Result<Option<IntegrationConnection>, UniversalInboxError> {
        let row = sqlx::query_as!(
            IntegrationConnectionRow,
            r#"
                SELECT
                  integration_connection.id,
                  integration_connection.user_id,
                  integration_connection.provider_user_id,
                  integration_connection.connection_id,
                  integration_connection.status as "status: _",
                  integration_connection.failure_message,
                  integration_connection.created_at,
                  integration_connection.updated_at,
                  integration_connection.last_notifications_sync_scheduled_at,
                  integration_connection.last_notifications_sync_started_at,
                  integration_connection.last_notifications_sync_completed_at,
                  integration_connection.last_notifications_sync_failed_at,
                  integration_connection.last_notifications_sync_failure_message,
                  integration_connection.notifications_sync_failures,
                  integration_connection.last_tasks_sync_scheduled_at,
                  integration_connection.last_tasks_sync_started_at,
                  integration_connection.last_tasks_sync_completed_at,
                  integration_connection.last_tasks_sync_failed_at,
                  integration_connection.last_tasks_sync_failure_message,
                  integration_connection.tasks_sync_failures,
                  integration_connection.first_notifications_sync_failed_at,
                  integration_connection.first_tasks_sync_failed_at,
                  integration_connection_config.config as "config: Json<IntegrationConnectionConfig>",
                  integration_connection.context as "context: Json<IntegrationConnectionContext>",
                  integration_connection.registered_oauth_scopes as "registered_oauth_scopes: Json<Vec<String>>"
                FROM integration_connection
                INNER JOIN integration_connection_config
                  ON integration_connection.id = integration_connection_config.integration_connection_id
                WHERE integration_connection.id = $1
            "#,
            integration_connection_id.0
        )
        .fetch_optional(&mut **executor)
        .await
            .map_err(|err| {
                let message = format!(
                    "Failed to fetch integration connection {integration_connection_id} from storage: {err}"
                );
                UniversalInboxError::DatabaseError { source: err, message }
        })?;

        row.map(|r| r.try_into()).transpose()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            user.id = user_id.to_string(),
            integration_provider_kind = integration_provider_kind.to_string(),
            synced_before_filter,
            with_status
        ),
        err
    )]
    async fn get_integration_connection_per_provider(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        integration_provider_kind: IntegrationProviderKind,
        synced_before_filter: Option<IntegrationConnectionSyncedBeforeFilter>,
        with_status: Option<IntegrationConnectionStatus>,
    ) -> Result<Option<IntegrationConnection>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
                SELECT
                  integration_connection.id,
                  integration_connection.user_id,
                  integration_connection.provider_user_id,
                  integration_connection.connection_id,
                  integration_connection.status,
                  integration_connection.failure_message,
                  integration_connection.created_at,
                  integration_connection.updated_at,
                  integration_connection.last_notifications_sync_scheduled_at,
                  integration_connection.last_notifications_sync_started_at,
                  integration_connection.last_notifications_sync_completed_at,
                  integration_connection.last_notifications_sync_failed_at,
                  integration_connection.last_notifications_sync_failure_message,
                  integration_connection.notifications_sync_failures,
                  integration_connection.last_tasks_sync_scheduled_at,
                  integration_connection.last_tasks_sync_started_at,
                  integration_connection.last_tasks_sync_completed_at,
                  integration_connection.last_tasks_sync_failed_at,
                  integration_connection.last_tasks_sync_failure_message,
                  integration_connection.tasks_sync_failures,
                  integration_connection.first_notifications_sync_failed_at,
                  integration_connection.first_tasks_sync_failed_at,
                  integration_connection_config.config as config,
                  integration_connection.context,
                  integration_connection.registered_oauth_scopes
                FROM integration_connection
                INNER JOIN integration_connection_config
                  ON integration_connection.id = integration_connection_config.integration_connection_id
                WHERE
            "#,
        );
        let mut separated = query_builder.separated(" AND ");
        separated
            .push("integration_connection.user_id = ")
            .push_bind_unseparated(user_id.0);
        separated
            .push("integration_connection.provider_kind::TEXT = ")
            .push_bind_unseparated(integration_provider_kind.to_string());

        match synced_before_filter {
            Some(IntegrationConnectionSyncedBeforeFilter::Notifications(synced_before)) => {
                separated
                    .push("(integration_connection.last_notifications_sync_started_at is null OR integration_connection.last_notifications_sync_started_at <= ")
                    .push_bind_unseparated(synced_before)
                    .push_unseparated(")");
            }
            Some(IntegrationConnectionSyncedBeforeFilter::Tasks(synced_before)) => {
                separated
                    .push("(integration_connection.last_tasks_sync_started_at is null OR integration_connection.last_tasks_sync_started_at <= ")
                    .push_bind_unseparated(synced_before)
                    .push_unseparated(")");
            }
            None => {}
        }

        if let Some(status) = with_status {
            separated
                .push("(integration_connection.status::TEXT = ")
                .push_bind_unseparated(status.to_string())
                .push_unseparated(")");
        }

        let row: Option<IntegrationConnectionRow> = query_builder
            .build_query_as::<IntegrationConnectionRow>()
            .fetch_optional(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!(
                    "Failed to fetch integration connection for user {user_id} of kind {integration_provider_kind} from storage: {err}"
                );
                UniversalInboxError::DatabaseError { source: err, message }
            })?;

        row.map(|r| r.try_into()).transpose()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_provider_kind = integration_provider_kind.to_string(),
            provider_user_id
        ),
        err
    )]
    async fn get_integration_connection_per_provider_user_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_provider_kind: IntegrationProviderKind,
        provider_user_id: String,
    ) -> Result<Option<IntegrationConnection>, UniversalInboxError> {
        let row = sqlx::query_as!(
            IntegrationConnectionRow,
            r#"
                SELECT
                  integration_connection.id,
                  integration_connection.user_id,
                  integration_connection.provider_user_id,
                  integration_connection.connection_id,
                  integration_connection.status as "status: _",
                  integration_connection.failure_message,
                  integration_connection.created_at,
                  integration_connection.updated_at,
                  integration_connection.last_notifications_sync_scheduled_at,
                  integration_connection.last_notifications_sync_started_at,
                  integration_connection.last_notifications_sync_completed_at,
                  integration_connection.last_notifications_sync_failed_at,
                  integration_connection.last_notifications_sync_failure_message,
                  integration_connection.notifications_sync_failures,
                  integration_connection.last_tasks_sync_scheduled_at,
                  integration_connection.last_tasks_sync_started_at,
                  integration_connection.last_tasks_sync_completed_at,
                  integration_connection.last_tasks_sync_failed_at,
                  integration_connection.last_tasks_sync_failure_message,
                  integration_connection.tasks_sync_failures,
                  integration_connection.first_notifications_sync_failed_at,
                  integration_connection.first_tasks_sync_failed_at,
                  integration_connection_config.config as "config: Json<IntegrationConnectionConfig>",
                  integration_connection.context as "context: Json<IntegrationConnectionContext>",
                  integration_connection.registered_oauth_scopes as "registered_oauth_scopes: Json<Vec<String>>"
                FROM integration_connection
                INNER JOIN integration_connection_config
                  ON integration_connection.id = integration_connection_config.integration_connection_id
                WHERE
                    integration_connection.provider_user_id = $1
                    AND integration_connection.provider_kind::TEXT = $2
                    AND integration_connection.status::TEXT = 'Validated'
            "#,
            provider_user_id,
            integration_provider_kind.to_string()
        )
            .fetch_optional(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!(
                    "Failed to fetch integration connection for {integration_provider_kind} user {provider_user_id} from storage: {err}"
                );
                UniversalInboxError::DatabaseError { source: err, message }
            })?;

        row.map(|r| r.try_into()).transpose()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_provider_kind = integration_provider_kind.to_string(),
            provider_user_ids
        ),
        err
    )]
    async fn find_integration_connection_per_provider_user_ids(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_provider_kind: IntegrationProviderKind,
        provider_user_ids: Vec<String>,
    ) -> Result<Vec<IntegrationConnection>, UniversalInboxError> {
        let rows = sqlx::query_as!(
            IntegrationConnectionRow,
            r#"
                SELECT
                  integration_connection.id,
                  integration_connection.user_id,
                  integration_connection.provider_user_id,
                  integration_connection.connection_id,
                  integration_connection.status as "status: _",
                  integration_connection.failure_message,
                  integration_connection.created_at,
                  integration_connection.updated_at,
                  integration_connection.last_notifications_sync_scheduled_at,
                  integration_connection.last_notifications_sync_started_at,
                  integration_connection.last_notifications_sync_completed_at,
                  integration_connection.last_notifications_sync_failed_at,
                  integration_connection.last_notifications_sync_failure_message,
                  integration_connection.notifications_sync_failures,
                  integration_connection.last_tasks_sync_scheduled_at,
                  integration_connection.last_tasks_sync_started_at,
                  integration_connection.last_tasks_sync_completed_at,
                  integration_connection.last_tasks_sync_failed_at,
                  integration_connection.last_tasks_sync_failure_message,
                  integration_connection.tasks_sync_failures,
                  integration_connection.first_notifications_sync_failed_at,
                  integration_connection.first_tasks_sync_failed_at,
                  integration_connection_config.config as "config: Json<IntegrationConnectionConfig>",
                  integration_connection.context as "context: Json<IntegrationConnectionContext>",
                  integration_connection.registered_oauth_scopes as "registered_oauth_scopes: Json<Vec<String>>"
                FROM integration_connection
                INNER JOIN integration_connection_config
                  ON integration_connection.id = integration_connection_config.integration_connection_id
                WHERE
                    integration_connection.provider_user_id = ANY($1)
                    AND integration_connection.provider_kind::TEXT = $2
                    AND integration_connection.status::TEXT = 'Validated'
            "#,
            &provider_user_ids[..],
            integration_provider_kind.to_string()
        )
            .fetch_all(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!(
                    "Failed to fetch integration connection for {integration_provider_kind} from storage: {err}"
                );
                UniversalInboxError::DatabaseError { source: err, message }
            })?;

        rows.iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<IntegrationConnection>, UniversalInboxError>>()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(context, required_oauth_scopes), err)]
    async fn get_integration_connection_per_context(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        context: IntegrationConnectionContext,
        required_oauth_scopes: &[String],
    ) -> Result<Option<IntegrationConnection>, UniversalInboxError> {
        let IntegrationConnectionContext::Slack(ref slack_context) = context else {
            return Err(UniversalInboxError::UnsupportedAction(format!(
                "Unsupported integration connection context: {context:?}"
            )));
        };

        let row = sqlx::query_as!(
            IntegrationConnectionRow,
            r#"
                SELECT
                  integration_connection.id,
                  integration_connection.user_id,
                  integration_connection.provider_user_id,
                  integration_connection.connection_id,
                  integration_connection.status as "status: _",
                  integration_connection.failure_message,
                  integration_connection.created_at,
                  integration_connection.updated_at,
                  integration_connection.last_notifications_sync_scheduled_at,
                  integration_connection.last_notifications_sync_started_at,
                  integration_connection.last_notifications_sync_completed_at,
                  integration_connection.last_notifications_sync_failed_at,
                  integration_connection.last_notifications_sync_failure_message,
                  integration_connection.notifications_sync_failures,
                  integration_connection.last_tasks_sync_scheduled_at,
                  integration_connection.last_tasks_sync_started_at,
                  integration_connection.last_tasks_sync_completed_at,
                  integration_connection.last_tasks_sync_failed_at,
                  integration_connection.last_tasks_sync_failure_message,
                  integration_connection.tasks_sync_failures,
                  integration_connection.first_notifications_sync_failed_at,
                  integration_connection.first_tasks_sync_failed_at,
                  integration_connection_config.config as "config: Json<IntegrationConnectionConfig>",
                  integration_connection.context as "context: Json<IntegrationConnectionContext>",
                  integration_connection.registered_oauth_scopes as "registered_oauth_scopes: Json<Vec<String>>"
                FROM integration_connection
                INNER JOIN integration_connection_config
                  ON integration_connection.id = integration_connection_config.integration_connection_id
                WHERE
                    integration_connection.context->'content'->>'team_id' = $1
                    AND integration_connection.provider_kind::TEXT = 'Slack'
                    AND integration_connection.status::TEXT = 'Validated'
                    AND integration_connection.registered_oauth_scopes::jsonb @> $2::jsonb
                LIMIT 1
            "#,
            slack_context.team_id.to_string(),
            Json(required_oauth_scopes) as _
        )
            .fetch_optional(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!(
                    "Failed to fetch Slack integration connection with context {context:?} from storage: {err}"
                );
                UniversalInboxError::DatabaseError { source: err, message }
            })?;

        row.map(|r| r.try_into()).transpose()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_connection_id = integration_connection_id.to_string(),
            new_status = new_status.to_string(),
            failure_message,
            registered_oauth_scopes,
            user.id = for_user_id.to_string()
        ),
        err
    )]
    async fn update_integration_connection_status(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        new_status: IntegrationConnectionStatus,
        failure_message: Option<String>,
        registered_oauth_scopes: Option<Vec<String>>,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new("UPDATE integration_connection SET");
        let mut separated = query_builder.separated(", ");
        separated
            .push(" status = ")
            .push_bind_unseparated(new_status.to_string())
            .push_unseparated("::integration_connection_status");
        separated
            .push(" failure_message = ")
            .push_bind_unseparated(failure_message.clone());

        if let Some(registered_oauth_scopes) = &registered_oauth_scopes {
            separated
                .push(" registered_oauth_scopes = ")
                .push_bind_unseparated(Json(registered_oauth_scopes));
        }

        query_builder
            .push(" FROM integration_connection_config ")
            .push(" WHERE ")
            .separated(" AND ")
            .push(" integration_connection_config.integration_connection_id = integration_connection.id ")
            .push(" integration_connection.id = ")
            .push_bind_unseparated(integration_connection_id.0)
            .push(" integration_connection.user_id = ")
            .push_bind_unseparated(for_user_id.0);

        query_builder.push(
            r#"
                RETURNING
                  integration_connection.id,
                  integration_connection.user_id,
                  integration_connection.provider_user_id,
                  integration_connection.connection_id,
                  integration_connection.status,
                  integration_connection.failure_message,
                  integration_connection.created_at,
                  integration_connection.updated_at,
                  integration_connection.last_notifications_sync_scheduled_at,
                  integration_connection.last_notifications_sync_started_at,
                  integration_connection.last_notifications_sync_completed_at,
                  integration_connection.last_notifications_sync_failed_at,
                  integration_connection.last_notifications_sync_failure_message,
                  integration_connection.notifications_sync_failures,
                  integration_connection.last_tasks_sync_scheduled_at,
                  integration_connection.last_tasks_sync_started_at,
                  integration_connection.last_tasks_sync_completed_at,
                  integration_connection.last_tasks_sync_failed_at,
                  integration_connection.last_tasks_sync_failure_message,
                  integration_connection.tasks_sync_failures,
                  integration_connection.first_notifications_sync_failed_at,
                  integration_connection.first_tasks_sync_failed_at,
                  integration_connection_config.config as config,
                  integration_connection.context,
                  integration_connection.registered_oauth_scopes,
                  (SELECT
             "#,
        );

        let mut separated = query_builder.separated(" OR ");
        separated
            .push(" status::TEXT != ")
            .push_bind_unseparated(new_status.to_string());
        if let Some(failure_message) = failure_message {
            separated
                .push(" (failure_message IS NULL OR failure_message != ")
                .push_bind_unseparated(failure_message)
                .push_unseparated(")");
        } else {
            separated.push(" failure_message IS NOT NULL");
        }

        if let Some(registered_oauth_scopes) = &registered_oauth_scopes {
            separated
                .push(" registered_oauth_scopes::jsonb != ")
                .push_bind_unseparated(Json(registered_oauth_scopes));
        }

        query_builder
            .push(" FROM integration_connection WHERE id = ")
            .push_bind(integration_connection_id.0)
            .push(r#") as "is_updated""#);

        let row: Option<UpdatedIntegrationConnectionRow> = query_builder
            .build_query_as::<UpdatedIntegrationConnectionRow>()
            .fetch_optional(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!(
                    "Failed to update integration connection {integration_connection_id} from storage: {err}"
                );
                UniversalInboxError::DatabaseError { source: err, message }
            })?;

        if let Some(updated_integration_connection_row) = row {
            Ok(UpdateStatus {
                updated: updated_integration_connection_row.is_updated,
                result: Some(Box::new(
                    updated_integration_connection_row
                        .integration_connection_row
                        .try_into()
                        .unwrap(),
                )),
            })
        } else {
            Ok(UpdateStatus {
                updated: false,
                result: None,
            })
        }
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            user.id = user_id.map(|x| x.to_string()),
            integration_provider_kind = integration_provider_kind.map(|x| x.to_string()),
            sync_update
        ),
        err
    )]
    async fn update_integration_connection_sync_status(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: Option<UserId>,
        integration_provider_kind: Option<IntegrationProviderKind>,
        sync_update: IntegrationConnectionSyncStatusUpdate,
        sync_failure_window_in_hours: i64,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new("UPDATE integration_connection SET");
        let mut separated = query_builder.separated(", ");
        match sync_update {
            IntegrationConnectionSyncStatusUpdate::NotificationsSyncScheduled => {
                separated
                    .push(" last_notifications_sync_scheduled_at = ")
                    .push_bind_unseparated(Utc::now());
            }
            IntegrationConnectionSyncStatusUpdate::NotificationsSyncStarted => {
                separated
                    .push(" last_notifications_sync_started_at = ")
                    .push_bind_unseparated(Utc::now());
            }
            IntegrationConnectionSyncStatusUpdate::NotificationsSyncCompleted => {
                separated
                    .push(" last_notifications_sync_completed_at = ")
                    .push_bind_unseparated(Utc::now());
                separated.push(" notifications_sync_failures = 0");
                separated.push(" last_notifications_sync_failure_message = NULL");
                separated.push(" last_notifications_sync_failed_at = NULL");
                separated.push(" first_notifications_sync_failed_at = NULL");
            }
            IntegrationConnectionSyncStatusUpdate::NotificationsSyncFailed(failure_message) => {
                separated
                    .push(" last_notifications_sync_failed_at = ")
                    .push_bind_unseparated(Utc::now());
                separated.push(" notifications_sync_failures = notifications_sync_failures + 1");
                separated
                    .push(" last_notifications_sync_failure_message = ")
                    .push_bind_unseparated(failure_message);
                separated.push(" first_notifications_sync_failed_at = COALESCE(first_notifications_sync_failed_at, now())");
                separated.push(format!(
                    " status = CASE WHEN first_notifications_sync_failed_at IS NOT NULL AND now() - first_notifications_sync_failed_at > interval '{sync_failure_window_in_hours} hours' THEN 'Failing' ELSE status END "
                ));
                separated.push(format!(
                    " failure_message = CASE WHEN first_notifications_sync_failed_at IS NOT NULL AND now() - first_notifications_sync_failed_at > interval '{sync_failure_window_in_hours} hours' THEN '{TOO_MANY_SYNC_FAILURES_ERROR_MESSAGE}' ELSE failure_message END "
                ));
            }
            IntegrationConnectionSyncStatusUpdate::TasksSyncScheduled => {
                separated
                    .push(" last_tasks_sync_scheduled_at = ")
                    .push_bind_unseparated(Utc::now());
            }
            IntegrationConnectionSyncStatusUpdate::TasksSyncStarted => {
                separated
                    .push(" last_tasks_sync_started_at = ")
                    .push_bind_unseparated(Utc::now());
            }
            IntegrationConnectionSyncStatusUpdate::TasksSyncCompleted => {
                separated
                    .push(" last_tasks_sync_completed_at = ")
                    .push_bind_unseparated(Utc::now());
                separated.push(" tasks_sync_failures = 0");
                separated.push(" last_tasks_sync_failure_message = NULL");
                separated.push(" last_tasks_sync_failed_at = NULL");
                separated.push(" first_tasks_sync_failed_at = NULL");
            }
            IntegrationConnectionSyncStatusUpdate::TasksSyncFailed(failure_message) => {
                separated
                    .push(" last_tasks_sync_failed_at = ")
                    .push_bind_unseparated(Utc::now());
                separated.push(" tasks_sync_failures = tasks_sync_failures + 1");
                separated
                    .push(" last_tasks_sync_failure_message = ")
                    .push_bind_unseparated(failure_message);
                separated.push(
                    " first_tasks_sync_failed_at = COALESCE(first_tasks_sync_failed_at, now())",
                );
                separated.push(format!(
                    " status = CASE WHEN first_tasks_sync_failed_at IS NOT NULL AND now() - first_tasks_sync_failed_at > interval '{sync_failure_window_in_hours} hours' THEN 'Failing' ELSE status END "
                ));
                separated.push(format!(
                    " failure_message = CASE WHEN first_tasks_sync_failed_at IS NOT NULL AND now() - first_tasks_sync_failed_at > interval '{sync_failure_window_in_hours} hours' THEN '{TOO_MANY_SYNC_FAILURES_ERROR_MESSAGE}' ELSE failure_message END "
                ));
            }
        }

        query_builder
            .push(" FROM integration_connection_config ")
            .push(" WHERE ");
        let mut separated = query_builder.separated(" AND ");
        separated.push(
            " integration_connection_config.integration_connection_id = integration_connection.id ",
        );
        if let Some(integration_provider_kind) = integration_provider_kind {
            separated
                .push(" integration_connection.provider_kind::TEXT = ")
                .push_bind_unseparated(integration_provider_kind.to_string());
        }
        if let Some(user_id) = user_id {
            separated
                .push(" integration_connection.user_id = ")
                .push_bind_unseparated(user_id.0);
        }

        query_builder.push(
            r#"
                RETURNING
                  integration_connection.id,
                  integration_connection.user_id,
                  integration_connection.provider_user_id,
                  integration_connection.connection_id,
                  integration_connection.status,
                  integration_connection.failure_message,
                  integration_connection.created_at,
                  integration_connection.updated_at,
                  integration_connection.last_notifications_sync_scheduled_at,
                  integration_connection.last_notifications_sync_started_at,
                  integration_connection.last_notifications_sync_completed_at,
                  integration_connection.last_notifications_sync_failed_at,
                  integration_connection.last_notifications_sync_failure_message,
                  integration_connection.notifications_sync_failures,
                  integration_connection.last_tasks_sync_scheduled_at,
                  integration_connection.last_tasks_sync_started_at,
                  integration_connection.last_tasks_sync_completed_at,
                  integration_connection.last_tasks_sync_failed_at,
                  integration_connection.last_tasks_sync_failure_message,
                  integration_connection.tasks_sync_failures,
                  integration_connection.first_notifications_sync_failed_at,
                  integration_connection.first_tasks_sync_failed_at,
                  integration_connection_config.config as config,
                  integration_connection.context,
                  integration_connection.registered_oauth_scopes,
                  true as "is_updated"
             "#,
        );

        let row: Option<UpdatedIntegrationConnectionRow> = query_builder
            .build_query_as::<UpdatedIntegrationConnectionRow>()
            .fetch_optional(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to update integration connection {integration_provider_kind:?} for user {user_id:?} from storage: {err}");
                UniversalInboxError::DatabaseError { source: err, message }
            })?;

        if let Some(updated_integration_connection_row) = row {
            Ok(UpdateStatus {
                updated: updated_integration_connection_row.is_updated,
                result: Some(Box::new(
                    updated_integration_connection_row
                        .integration_connection_row
                        .try_into()
                        .unwrap(),
                )),
            })
        } else {
            Ok(UpdateStatus {
                updated: false,
                result: None,
            })
        }
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_connection_id = integration_connection_id.to_string(),
            context
        ),
        err
    )]
    async fn update_integration_connection_context(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        context: Option<IntegrationConnectionContext>,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new("UPDATE integration_connection SET context = ");
        query_builder
            .push_bind(context.map(Json))
            .push(" FROM integration_connection_config ")
            .push(" WHERE ")
            .separated(" AND ")
            .push(" integration_connection_config.integration_connection_id = integration_connection.id ")
            .push(" integration_connection.id = ")
            .push_bind_unseparated(integration_connection_id.0);

        query_builder.push(
            r#"
                RETURNING
                  integration_connection.id,
                  integration_connection.user_id,
                  integration_connection.provider_user_id,
                  integration_connection.connection_id,
                  integration_connection.status,
                  integration_connection.failure_message,
                  integration_connection.created_at,
                  integration_connection.updated_at,
                  integration_connection.last_notifications_sync_scheduled_at,
                  integration_connection.last_notifications_sync_started_at,
                  integration_connection.last_notifications_sync_completed_at,
                  integration_connection.last_notifications_sync_failed_at,
                  integration_connection.last_notifications_sync_failure_message,
                  integration_connection.notifications_sync_failures,
                  integration_connection.last_tasks_sync_scheduled_at,
                  integration_connection.last_tasks_sync_started_at,
                  integration_connection.last_tasks_sync_completed_at,
                  integration_connection.last_tasks_sync_failed_at,
                  integration_connection.last_tasks_sync_failure_message,
                  integration_connection.tasks_sync_failures,
                  integration_connection.first_notifications_sync_failed_at,
                  integration_connection.first_tasks_sync_failed_at,
                  integration_connection_config.config as config,
                  integration_connection.context,
                  integration_connection.registered_oauth_scopes,
                  true as "is_updated"
               "#,
        );

        let row: Option<UpdatedIntegrationConnectionRow> = query_builder
            .build_query_as::<UpdatedIntegrationConnectionRow>()
            .fetch_optional(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!(
                    "Failed to update integration connection {integration_connection_id} context from storage: {err}"
                );
                UniversalInboxError::DatabaseError { source: err, message }
            })?;

        if let Some(updated_integration_connection_row) = row {
            Ok(UpdateStatus {
                updated: updated_integration_connection_row.is_updated,
                result: Some(Box::new(
                    updated_integration_connection_row
                        .integration_connection_row
                        .try_into()
                        .unwrap(),
                )),
            })
        } else {
            Ok(UpdateStatus {
                updated: false,
                result: None,
            })
        }
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            user.id = for_user_id.to_string(),
            status,
            lock_rows
        ),
        err
    )]
    async fn fetch_all_integration_connections(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        for_user_id: UserId,
        status: Option<IntegrationConnectionStatus>,
        lock_rows: bool,
    ) -> Result<Vec<IntegrationConnection>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
                SELECT
                  integration_connection.id,
                  integration_connection.user_id,
                  integration_connection.provider_user_id,
                  integration_connection.connection_id,
                  integration_connection.status,
                  integration_connection.failure_message,
                  integration_connection.created_at,
                  integration_connection.updated_at,
                  integration_connection.last_notifications_sync_scheduled_at,
                  integration_connection.last_notifications_sync_started_at,
                  integration_connection.last_notifications_sync_completed_at,
                  integration_connection.last_notifications_sync_failed_at,
                  integration_connection.last_notifications_sync_failure_message,
                  integration_connection.notifications_sync_failures,
                  integration_connection.last_tasks_sync_scheduled_at,
                  integration_connection.last_tasks_sync_started_at,
                  integration_connection.last_tasks_sync_completed_at,
                  integration_connection.last_tasks_sync_failed_at,
                  integration_connection.last_tasks_sync_failure_message,
                  integration_connection.tasks_sync_failures,
                  integration_connection.first_notifications_sync_failed_at,
                  integration_connection.first_tasks_sync_failed_at,
                  integration_connection_config.config,
                  integration_connection.context,
                  integration_connection.registered_oauth_scopes
                FROM integration_connection
                INNER JOIN integration_connection_config
                  ON integration_connection.id = integration_connection_config.integration_connection_id
                WHERE
            "#,
        );
        let mut separated = query_builder.separated(" AND ");
        separated
            .push("user_id = ")
            .push_bind_unseparated(for_user_id.0);
        if let Some(status) = status {
            separated
                .push("integration_connection.status::TEXT = ")
                .push_bind_unseparated(status.to_string());
        }
        if lock_rows {
            query_builder.push(" FOR UPDATE ");
        }

        let rows = query_builder
            .build_query_as::<IntegrationConnectionRow>()
            .fetch_all(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!(
                    "Failed to fetch all integration connections for user {for_user_id} from storage: {err}"
                );
                UniversalInboxError::DatabaseError { source: err, message }
            })?;

        rows.into_iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<IntegrationConnection>, UniversalInboxError>>()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(integration_connection_id = integration_connection.id.to_string()),
        err
    )]
    async fn create_integration_connection(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection: Box<IntegrationConnection>,
    ) -> Result<Box<IntegrationConnection>, UniversalInboxError> {
        debug!("Creating integration connection: {integration_connection:?}");
        sqlx::query!(
            r#"
                INSERT INTO integration_connection
                  (
                    id,
                    user_id,
                    connection_id,
                    provider_kind,
                    status,
                    failure_message,
                    notifications_sync_failures,
                    tasks_sync_failures,
                    first_notifications_sync_failed_at,
                    first_tasks_sync_failed_at,
                    created_at,
                    updated_at
                  )
                VALUES
                  (
                    $1,
                    $2,
                    $3,
                    $4::integration_provider_kind,
                    $5::integration_connection_status,
                    $6,
                    $7,
                    $8,
                    $9,
                    $10,
                    $11,
                    $12
                  )
            "#,
            integration_connection.id.0,
            integration_connection.user_id.0,
            integration_connection.connection_id.0,
            integration_connection.provider.kind().to_string() as _,
            integration_connection.status.to_string() as _,
            integration_connection.failure_message,
            integration_connection.notifications_sync_failures as i32,
            integration_connection.tasks_sync_failures as i32,
            integration_connection
                .first_notifications_sync_failed_at
                .map(|t| t.naive_utc()),
            integration_connection
                .first_tasks_sync_failed_at
                .map(|t| t.naive_utc()),
            integration_connection.created_at.naive_utc(),
            integration_connection.updated_at.naive_utc()
        )
        .execute(&mut **executor)
        .await
        .map_err(|e| {
            match e
                .as_database_error()
                .and_then(|db_error| db_error.code().map(|code| code.to_string()))
            {
                Some(x) if x == *"23505" => UniversalInboxError::AlreadyExists {
                    source: Some(e),
                    id: integration_connection.id.0,
                },
                _ => UniversalInboxError::Unexpected(anyhow!(
                    "Failed to insert new integration connection into storage: {e}"
                )),
            }
        })?;

        let now = Utc::now().naive_utc();
        let new_id = Uuid::new_v4();
        sqlx::query!(
            r#"
                INSERT INTO integration_connection_config
                  (
                    id,
                    integration_connection_id,
                    config,
                    created_at,
                    updated_at
                  )
                VALUES
                  (
                    $1,
                    $2,
                    $3,
                    $4,
                    $5
                  )
            "#,
            new_id,
            integration_connection.id.0,
            Json(integration_connection.provider.config()) as Json<IntegrationConnectionConfig>,
            now,
            now,
        )
        .execute(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!(
                "Failed to insert configuration for integration connection {} into storage: {err}",
                integration_connection.id
            );
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        Ok(integration_connection)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(integration_connection_id = id.to_string()),
        err
    )]
    async fn does_integration_connection_exist(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: IntegrationConnectionId,
    ) -> Result<bool, UniversalInboxError> {
        let exists: Option<bool> = sqlx::query_scalar!(
            r#"SELECT EXISTS(SELECT 1 FROM integration_connection WHERE id = $1)"#,
            id.0
        )
        .fetch_one(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!("Failed to check if integration connection {id} exists: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        Ok(exists.unwrap_or(false))
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_connection_id = integration_connection_id.to_string(),
            config,
            user.id = for_user_id.to_string()
        ),
        err
    )]
    async fn update_integration_connection_config(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        config: IntegrationConnectionConfig,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<IntegrationConnectionConfig>>, UniversalInboxError> {
        let mut query_builder =
            QueryBuilder::new("UPDATE integration_connection_config SET config = ");
        query_builder
            .push_bind(Json(config.clone()))
            .push(" FROM integration_connection ")
            .push(" WHERE ")
            .separated(" AND ")
            .push(" integration_connection.id = integration_connection_config.integration_connection_id ")
            .push(" integration_connection.id = ")
            .push_bind_unseparated(integration_connection_id.0)
            .push(" integration_connection.user_id = ")
            .push_bind_unseparated(for_user_id.0);

        query_builder.push(
            r#"
                RETURNING
                  integration_connection.id,
                  integration_connection.user_id,
                  integration_connection.provider_user_id,
                  integration_connection.connection_id,
                  integration_connection.status,
                  integration_connection.failure_message,
                  integration_connection.created_at,
                  integration_connection.updated_at,
                  integration_connection.last_notifications_sync_scheduled_at,
                  integration_connection.last_notifications_sync_started_at,
                  integration_connection.last_notifications_sync_completed_at,
                  integration_connection.last_notifications_sync_failed_at,
                  integration_connection.last_notifications_sync_failure_message,
                  integration_connection.notifications_sync_failures,
                  integration_connection.last_tasks_sync_scheduled_at,
                  integration_connection.last_tasks_sync_started_at,
                  integration_connection.last_tasks_sync_completed_at,
                  integration_connection.last_tasks_sync_failed_at,
                  integration_connection.last_tasks_sync_failure_message,
                  integration_connection.tasks_sync_failures,
                  integration_connection.first_notifications_sync_failed_at,
                  integration_connection.first_tasks_sync_failed_at,
                  integration_connection_config.config as config,
                  integration_connection.context,
                  integration_connection.registered_oauth_scopes,
                  true as "is_updated"
               "#,
        );

        let row: Option<UpdatedIntegrationConnectionRow> = query_builder
            .build_query_as::<UpdatedIntegrationConnectionRow>()
            .fetch_optional(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!(
                    "Failed to update integration connection {integration_connection_id} config from storage: {err}"
                );
                UniversalInboxError::DatabaseError { source: err, message }
            })?;

        if let Some(updated_integration_connection_row) = row {
            Ok(UpdateStatus {
                updated: updated_integration_connection_row.is_updated,
                result: Some(Box::new(config)),
            })
        } else {
            Ok(UpdateStatus {
                updated: false,
                result: None,
            })
        }
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_connection_id = integration_connection_id.to_string(),
            provider_user_id
        )
    )]
    async fn update_integration_connection_provider_user_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        provider_user_id: Option<String>,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError> {
        let mut query_builder =
            QueryBuilder::new("UPDATE integration_connection SET provider_user_id = ");
        query_builder
            .push_bind(provider_user_id)
            .push(" FROM integration_connection_config ")
            .push(" WHERE ")
            .separated(" AND ")
            .push(" integration_connection_config.integration_connection_id = integration_connection.id ")
            .push(" integration_connection.id = ")
            .push_bind_unseparated(integration_connection_id.0);

        query_builder.push(
            r#"
                RETURNING
                  integration_connection.id,
                  integration_connection.user_id,
                  integration_connection.provider_user_id,
                  integration_connection.connection_id,
                  integration_connection.status,
                  integration_connection.failure_message,
                  integration_connection.created_at,
                  integration_connection.updated_at,
                  integration_connection.last_notifications_sync_scheduled_at,
                  integration_connection.last_notifications_sync_started_at,
                  integration_connection.last_notifications_sync_completed_at,
                  integration_connection.last_notifications_sync_failed_at,
                  integration_connection.last_notifications_sync_failure_message,
                  integration_connection.notifications_sync_failures,
                  integration_connection.last_tasks_sync_scheduled_at,
                  integration_connection.last_tasks_sync_started_at,
                  integration_connection.last_tasks_sync_completed_at,
                  integration_connection.last_tasks_sync_failed_at,
                  integration_connection.last_tasks_sync_failure_message,
                  integration_connection.tasks_sync_failures,
                  integration_connection.first_notifications_sync_failed_at,
                  integration_connection.first_tasks_sync_failed_at,
                  integration_connection_config.config as config,
                  integration_connection.context,
                  integration_connection.registered_oauth_scopes,
                  true as "is_updated"
               "#,
        );

        let row: Option<UpdatedIntegrationConnectionRow> = query_builder
            .build_query_as::<UpdatedIntegrationConnectionRow>()
            .fetch_optional(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!(
                    "Failed to update integration connection {integration_connection_id} provider user id from storage: {err}"
                );
                UniversalInboxError::DatabaseError { source: err, message }
            })?;

        if let Some(updated_integration_connection_row) = row {
            Ok(UpdateStatus {
                updated: updated_integration_connection_row.is_updated,
                result: Some(Box::new(
                    updated_integration_connection_row
                        .integration_connection_row
                        .try_into()
                        .unwrap(),
                )),
            })
        } else {
            Ok(UpdateStatus {
                updated: false,
                result: None,
            })
        }
    }
}

#[derive(sqlx::Type, Debug)]
#[sqlx(type_name = "integration_connection_status")]
enum PgIntegrationConnectionStatus {
    Created,
    Validated,
    Failing,
}

#[derive(Debug, sqlx::FromRow)]
pub struct IntegrationConnectionRow {
    id: Uuid,
    user_id: Uuid,
    provider_user_id: Option<String>,
    connection_id: Uuid,
    status: PgIntegrationConnectionStatus,
    failure_message: Option<String>,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    last_notifications_sync_scheduled_at: Option<NaiveDateTime>,
    last_notifications_sync_started_at: Option<NaiveDateTime>,
    last_notifications_sync_completed_at: Option<NaiveDateTime>,
    last_notifications_sync_failed_at: Option<NaiveDateTime>,
    last_notifications_sync_failure_message: Option<String>,
    notifications_sync_failures: i32,
    last_tasks_sync_scheduled_at: Option<NaiveDateTime>,
    last_tasks_sync_started_at: Option<NaiveDateTime>,
    last_tasks_sync_completed_at: Option<NaiveDateTime>,
    last_tasks_sync_failed_at: Option<NaiveDateTime>,
    last_tasks_sync_failure_message: Option<String>,
    tasks_sync_failures: i32,
    first_notifications_sync_failed_at: Option<NaiveDateTime>,
    first_tasks_sync_failed_at: Option<NaiveDateTime>,
    config: Json<IntegrationConnectionConfig>,
    context: Option<Json<IntegrationConnectionContext>>,
    registered_oauth_scopes: Json<Vec<String>>,
}

#[derive(Debug, sqlx::FromRow)]
struct UpdatedIntegrationConnectionRow {
    #[sqlx(flatten)]
    pub integration_connection_row: IntegrationConnectionRow,
    pub is_updated: bool,
}

impl TryFrom<&PgIntegrationConnectionStatus> for IntegrationConnectionStatus {
    type Error = UniversalInboxError;

    fn try_from(status: &PgIntegrationConnectionStatus) -> Result<Self, Self::Error> {
        let status_str = format!("{status:?}");
        status_str
            .parse()
            .map_err(|e| UniversalInboxError::InvalidEnumData {
                source: e,
                output: status_str,
            })
    }
}

impl TryFrom<IntegrationConnectionRow> for IntegrationConnection {
    type Error = UniversalInboxError;

    fn try_from(row: IntegrationConnectionRow) -> Result<Self, Self::Error> {
        (&row).try_into()
    }
}

impl TryFrom<&IntegrationConnectionRow> for IntegrationConnection {
    type Error = UniversalInboxError;
    fn try_from(row: &IntegrationConnectionRow) -> Result<Self, Self::Error> {
        let status = (&row.status).try_into()?;

        Ok(IntegrationConnection {
            id: row.id.into(),
            user_id: row.user_id.into(),
            provider_user_id: row.provider_user_id.clone(),
            connection_id: row.connection_id.into(),
            status,
            failure_message: row.failure_message.clone(),
            created_at: DateTime::from_naive_utc_and_offset(row.created_at, Utc),
            updated_at: DateTime::from_naive_utc_and_offset(row.updated_at, Utc),
            last_notifications_sync_scheduled_at: row
                .last_notifications_sync_scheduled_at
                .map(|scheduled_at| DateTime::from_naive_utc_and_offset(scheduled_at, Utc)),
            last_notifications_sync_started_at: row
                .last_notifications_sync_started_at
                .map(|started_at| DateTime::from_naive_utc_and_offset(started_at, Utc)),
            last_notifications_sync_completed_at: row
                .last_notifications_sync_completed_at
                .map(|completed_at| DateTime::from_naive_utc_and_offset(completed_at, Utc)),
            last_notifications_sync_failed_at: row
                .last_notifications_sync_failed_at
                .map(|failed_at| DateTime::from_naive_utc_and_offset(failed_at, Utc)),
            last_notifications_sync_failure_message: row
                .last_notifications_sync_failure_message
                .clone(),
            notifications_sync_failures: row.notifications_sync_failures as u32,
            last_tasks_sync_scheduled_at: row
                .last_tasks_sync_scheduled_at
                .map(|scheduled_at| DateTime::from_naive_utc_and_offset(scheduled_at, Utc)),
            last_tasks_sync_started_at: row
                .last_tasks_sync_started_at
                .map(|started_at| DateTime::from_naive_utc_and_offset(started_at, Utc)),
            last_tasks_sync_completed_at: row
                .last_tasks_sync_completed_at
                .map(|completed_at| DateTime::from_naive_utc_and_offset(completed_at, Utc)),
            last_tasks_sync_failed_at: row
                .last_tasks_sync_failed_at
                .map(|failed_at| DateTime::from_naive_utc_and_offset(failed_at, Utc)),
            last_tasks_sync_failure_message: row.last_tasks_sync_failure_message.clone(),
            tasks_sync_failures: row.tasks_sync_failures as u32,
            first_notifications_sync_failed_at: row
                .first_notifications_sync_failed_at
                .map(|t| DateTime::from_naive_utc_and_offset(t, Utc)),
            first_tasks_sync_failed_at: row
                .first_tasks_sync_failed_at
                .map(|t| DateTime::from_naive_utc_and_offset(t, Utc)),
            provider: IntegrationProvider::new(
                row.config.0.clone(),
                row.context.as_ref().map(|context| context.0.clone()),
            )?,
            registered_oauth_scopes: row.registered_oauth_scopes.0.clone(),
        })
    }
}
