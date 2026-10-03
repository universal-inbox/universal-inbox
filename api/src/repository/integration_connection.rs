use anyhow::anyhow;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::{Postgres, QueryBuilder, Transaction, types::Json};
use tracing::debug;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionId, IntegrationConnectionPausedReason,
        IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        provider::{IntegrationConnectionContext, IntegrationProvider, IntegrationProviderKind},
    },
    user::UserId,
};

use crate::observability::attr;
use crate::{
    repository::Repository,
    universal_inbox::{UniversalInboxError, UpdateStatus},
};

#[derive(Debug)]
pub enum IntegrationConnectionSyncStatusUpdate {
    NotificationsSyncScheduled,
    NotificationsSyncCompleted,
    NotificationsSyncFailed(String),
    TasksSyncScheduled,
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

    /// Every Slack connection of the workspace `team_id`, whatever its status,
    /// optionally narrowed to the given Slack user ids.
    async fn find_slack_integration_connections_per_team(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        team_id: &str,
        provider_user_ids: Option<&[String]>,
    ) -> Result<Vec<IntegrationConnection>, UniversalInboxError>;

    /// Slack connections whose `context` (hence `team_id`) was never stored,
    /// e.g. connected before `SlackContext` was captured from the OAuth response.
    async fn find_slack_integration_connections_without_context(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        for_user_id: Option<UserId>,
    ) -> Result<Vec<SlackIntegrationConnectionWithoutContext>, UniversalInboxError>;

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

    /// Atomically claims every one of `for_user_id`'s `Validated` connections (among
    /// `provider_kinds`) whose notifications sync is due, stamping
    /// `last_notifications_sync_scheduled_at = now` and returning only the rows it claimed.
    /// `FOR UPDATE SKIP LOCKED` makes two overlapping callers non-blocking by construction: a
    /// connection someone else is concurrently claiming is simply skipped, not waited on, so
    /// this can never deadlock and never holds a lock beyond this one short statement.
    async fn claim_due_notification_syncs(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        for_user_id: UserId,
        provider_kinds: &[IntegrationProviderKind],
        now: DateTime<Utc>,
        synced_before: DateTime<Utc>,
    ) -> Result<Vec<(IntegrationConnectionId, IntegrationProviderKind)>, UniversalInboxError>;

    /// Tasks counterpart of [`Self::claim_due_notification_syncs`].
    async fn claim_due_task_syncs(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        for_user_id: UserId,
        provider_kinds: &[IntegrationProviderKind],
        now: DateTime<Utc>,
        synced_before: DateTime<Utc>,
    ) -> Result<Vec<(IntegrationConnectionId, IntegrationProviderKind)>, UniversalInboxError>;

    /// Atomically claims the start of a notifications sync for one connection: stamps
    /// `last_notifications_sync_started_at = now` and returns `true`, unless
    /// `synced_before` is given and the connection was already (re)started more recently
    /// than that, in which case it does nothing and returns `false` — another worker (or
    /// this one, racing itself) already has this sync in flight. `synced_before: None`
    /// unconditionally (re)claims, matching `force_sync`.
    ///
    /// A single-row `UPDATE ... WHERE id = $id` is already atomic with respect to any
    /// concurrent caller — no explicit locking clause needed.
    async fn claim_notification_sync_start(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        now: DateTime<Utc>,
        synced_before: Option<DateTime<Utc>>,
    ) -> Result<bool, UniversalInboxError>;

    /// Tasks counterpart of [`Self::claim_notification_sync_start`].
    async fn claim_task_sync_start(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        now: DateTime<Utc>,
        synced_before: Option<DateTime<Utc>>,
    ) -> Result<bool, UniversalInboxError>;

    /// Count a user's *active* `Validated` integration connections without
    /// loading the rows or their JSON configs. Used by the billing cap check,
    /// which only ever needs the number. Connections already paused by the
    /// plan are excluded — see the query for why.
    async fn count_validated_integration_connections(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        for_user_id: UserId,
    ) -> Result<u32, UniversalInboxError>;

    /// Counterpart of [`Self::count_validated_integration_connections`]: how
    /// many of a user's connections the plan has paused. Drives the "N
    /// integrations are paused by your Free plan" state in the UI.
    async fn count_plan_paused_integration_connections(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        for_user_id: UserId,
    ) -> Result<u32, UniversalInboxError>;

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

    /// Set or clear the plan-pause state in a single UPDATE, keeping the
    /// `auto_paused_by_plan_at IS NOT NULL ⟺ auto_paused_config_snapshot IS NOT NULL`
    /// invariant atomic. The marker distinguishes plan-initiated pauses from
    /// user-initiated ones; the snapshot captures the connection's full config
    /// at pause time so the upgrade restore is byte-for-byte. Passing
    /// `paused_at = None` clears both (e.g. when the user re-upgrades to Paid);
    /// callers must pass `snapshot = None` in that case so the pair stays
    /// consistent.
    async fn set_integration_connection_plan_pause(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        paused_at: Option<chrono::DateTime<chrono::Utc>>,
        snapshot: Option<&IntegrationConnectionConfig>,
    ) -> Result<(), UniversalInboxError>;

    async fn update_integration_connection_provider_user_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        provider_user_id: Option<String>,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError>;

    /// List the `Validated` connections of `provider_kinds`, with their owner,
    /// whose owner has not been active since `inactive_before`. With `warned_before`, only those
    /// whose owner was warned of the pause before `warned_before`, and not
    /// active since (see [`Self::mark_inactivity_warning_sent`]).
    async fn find_validated_integration_connections_of_inactive_users(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        provider_kinds: &[IntegrationProviderKind],
        inactive_before: DateTime<Utc>,
        warned_before: Option<DateTime<Utc>>,
    ) -> Result<Vec<(IntegrationConnectionId, UserId)>, UniversalInboxError>;

    /// List the `Validated` connections of `provider_kinds`, with their owner,
    /// whose owner has not been active since `inactive_before` and was not warned yet of their
    /// pause since their last activity.
    async fn find_integration_connections_to_warn_of_inactivity(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        provider_kinds: &[IntegrationProviderKind],
        inactive_before: DateTime<Utc>,
    ) -> Result<Vec<(IntegrationConnectionId, UserId)>, UniversalInboxError>;

    /// Record that the owner of the connection was warned of its pause, if it
    /// is still to be warned (see
    /// [`Self::find_integration_connections_to_warn_of_inactivity`]). The row
    /// stays locked until the transaction ends. Returns whether it was marked.
    async fn mark_inactivity_warning_sent(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        inactive_before: DateTime<Utc>,
        sent_at: DateTime<Utc>,
    ) -> Result<bool, UniversalInboxError>;

    /// List the `Failing` connections of `provider_kinds`, with their owner,
    /// failing since before
    /// `failing_before`. A connection marked `Failing` by its syncs is failing
    /// since its first failed sync; one marked `Failing` by a token refresh
    /// has no such timestamp and falls back to its last update.
    async fn find_long_failing_integration_connections(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        provider_kinds: &[IntegrationProviderKind],
        failing_before: DateTime<Utc>,
    ) -> Result<Vec<(IntegrationConnectionId, UserId)>, UniversalInboxError>;

    /// Move a connection to `Paused`, setting `paused_at` and `paused_reason`
    /// together and clearing any failure message.
    async fn pause_integration_connection(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        paused_at: DateTime<Utc>,
        paused_reason: IntegrationConnectionPausedReason,
    ) -> Result<Option<IntegrationConnection>, UniversalInboxError>;
}

pub const TOO_MANY_SYNC_FAILURES_ERROR_MESSAGE: &str = "♻️ Synchronization has been failing for too long. Please try to reconnect the integration. If the issue keeps happening, please contact our support.";

pub const OAUTH_INVALID_GRANT_ERROR_MESSAGE: &str =
    "🔌 Authorization has expired or been revoked. Please reconnect this integration.";

pub const SLACK_ACCESS_REVOKED_ERROR_MESSAGE: &str =
    "🔌 Slack access was revoked. Please reconnect this integration.";

pub const OAUTH_MISSING_REFRESH_TOKEN_ERROR_MESSAGE: &str =
    "🔌 Authorization is missing a refresh token. Please reconnect this integration.";

#[derive(Debug, Clone, PartialEq)]
pub struct SlackIntegrationConnectionWithoutContext {
    pub id: IntegrationConnectionId,
    pub user_id: UserId,
    pub status: IntegrationConnectionStatus,
    pub has_credential: bool,
}

#[derive(sqlx::FromRow)]
struct SlackIntegrationConnectionWithoutContextRow {
    id: Uuid,
    user_id: Uuid,
    status: String,
    has_credential: bool,
}

#[derive(sqlx::FromRow)]
struct ClaimedIntegrationConnectionRow {
    id: Uuid,
    provider_kind: String,
}

/// Shared implementation for `claim_due_notification_syncs`/`claim_due_task_syncs`. Atomically
/// claims and stamps every `Validated` connection (among `provider_kinds`) belonging to
/// `for_user_id` whose `scheduled_at_column` is either unset or older than `synced_before`.
///
/// `scheduled_at_column` is one of two hardcoded literals chosen by the caller (never
/// user input), interpolated as a column identifier since SQL has no way to bind an
/// identifier as a parameter.
///
/// `FOR UPDATE SKIP LOCKED` is what makes this safe to call from a live HTTP request: a
/// connection another transaction is already touching (a running sync claiming/updating it,
/// or another concurrent claim) is simply left out of the result rather than blocked on —
/// this statement never waits on a lock, so it can never be a party to a deadlock, and it
/// holds its own claimed-row locks only for the remainder of this one short transaction.
async fn claim_due_syncs(
    executor: &mut Transaction<'_, Postgres>,
    scheduled_at_column: &'static str,
    for_user_id: UserId,
    provider_kinds: &[IntegrationProviderKind],
    now: DateTime<Utc>,
    synced_before: DateTime<Utc>,
) -> Result<Vec<(IntegrationConnectionId, IntegrationProviderKind)>, UniversalInboxError> {
    if provider_kinds.is_empty() {
        return Ok(vec![]);
    }

    let provider_kind_strings: Vec<String> =
        provider_kinds.iter().map(|kind| kind.to_string()).collect();
    let validated_status = IntegrationConnectionStatus::Validated.to_string();

    let mut query_builder = QueryBuilder::new(format!(
        "UPDATE integration_connection SET {scheduled_at_column} = "
    ));
    query_builder
        .push_bind(now)
        .push(" WHERE id IN ( SELECT id FROM integration_connection WHERE user_id = ")
        .push_bind(for_user_id.0)
        .push(" AND status::TEXT = ")
        .push_bind(validated_status)
        .push(" AND provider_kind::TEXT = ANY(")
        .push_bind(provider_kind_strings)
        .push(format!(
            ") AND ({scheduled_at_column} IS NULL OR {scheduled_at_column} <= "
        ))
        .push_bind(synced_before)
        .push(" ) ORDER BY id FOR UPDATE SKIP LOCKED ) RETURNING id, provider_kind::TEXT AS provider_kind");

    let rows: Vec<ClaimedIntegrationConnectionRow> = query_builder
        .build_query_as()
        .fetch_all(&mut **executor)
        .await
        .map_err(|err| {
            let message =
                format!("Failed to claim due syncs for user {for_user_id} from storage: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

    rows.into_iter()
        .map(|row| {
            let provider_kind =
                row.provider_kind
                    .parse()
                    .map_err(|e| UniversalInboxError::InvalidEnumData {
                        source: e,
                        output: row.provider_kind,
                    })?;
            Ok((IntegrationConnectionId(row.id), provider_kind))
        })
        .collect::<Result<Vec<_>, UniversalInboxError>>()
}

/// Shared implementation for `claim_notification_sync_start`/`claim_task_sync_start`. A
/// single-row `UPDATE ... WHERE id = $id [AND started_at is-due]` is already atomic — no
/// explicit locking clause is needed, unlike `claim_due_syncs` above which can touch several
/// rows at once.
async fn claim_sync_start(
    executor: &mut Transaction<'_, Postgres>,
    started_at_column: &'static str,
    integration_connection_id: IntegrationConnectionId,
    now: DateTime<Utc>,
    synced_before: Option<DateTime<Utc>>,
) -> Result<bool, UniversalInboxError> {
    let mut query_builder = QueryBuilder::new(format!(
        "UPDATE integration_connection SET {started_at_column} = "
    ));
    query_builder
        .push_bind(now)
        .push(" WHERE id = ")
        .push_bind(integration_connection_id.0);
    if let Some(synced_before) = synced_before {
        query_builder
            .push(format!(
                " AND ({started_at_column} IS NULL OR {started_at_column} <= "
            ))
            .push_bind(synced_before)
            .push(" )");
    }
    query_builder.push(" RETURNING id");

    let claimed_id: Option<Uuid> = query_builder
        .build_query_scalar()
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!(
                "Failed to claim sync start for integration connection {integration_connection_id} from storage: {err}"
            );
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

    Ok(claimed_id.is_some())
}

#[async_trait]
impl IntegrationConnectionRepository for Repository {
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::INTEGRATION_CONNECTION_ID } = integration_connection_id.to_string().to_string())
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
                  integration_connection.registered_oauth_scopes as "registered_oauth_scopes: Json<Vec<String>>",
                  integration_connection.auto_paused_by_plan_at,
                  integration_connection.auto_paused_config_snapshot as "auto_paused_config_snapshot: Json<IntegrationConnectionConfig>",
                  integration_connection.paused_at,
                  integration_connection.paused_reason
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
                  integration_connection.auto_paused_by_plan_at,
                  integration_connection.auto_paused_config_snapshot as auto_paused_config_snapshot,
                  integration_connection.paused_at,
                  integration_connection.paused_reason
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
            { attr::INTEGRATION_PROVIDER_KIND } = integration_provider_kind.to_string()
        )
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
                  integration_connection.registered_oauth_scopes as "registered_oauth_scopes: Json<Vec<String>>",
                  integration_connection.auto_paused_by_plan_at,
                  integration_connection.auto_paused_config_snapshot as "auto_paused_config_snapshot: Json<IntegrationConnectionConfig>",
                  integration_connection.paused_at,
                  integration_connection.paused_reason
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
            { attr::INTEGRATION_PROVIDER_KIND } = integration_provider_kind.to_string()
        )
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
                  integration_connection.registered_oauth_scopes as "registered_oauth_scopes: Json<Vec<String>>",
                  integration_connection.auto_paused_by_plan_at,
                  integration_connection.auto_paused_config_snapshot as "auto_paused_config_snapshot: Json<IntegrationConnectionConfig>",
                  integration_connection.paused_at,
                  integration_connection.paused_reason
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

    #[tracing::instrument(level = "debug", skip_all)]
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
                  integration_connection.registered_oauth_scopes as "registered_oauth_scopes: Json<Vec<String>>",
                  integration_connection.auto_paused_by_plan_at,
                  integration_connection.auto_paused_config_snapshot as "auto_paused_config_snapshot: Json<IntegrationConnectionConfig>",
                  integration_connection.paused_at,
                  integration_connection.paused_reason
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

    #[tracing::instrument(level = "debug", skip_all, fields({ attr::SLACK_TEAM_ID } = %team_id))]
    async fn find_slack_integration_connections_per_team(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        team_id: &str,
        provider_user_ids: Option<&[String]>,
    ) -> Result<Vec<IntegrationConnection>, UniversalInboxError> {
        let rows = sqlx::query_as!(
            IntegrationConnectionRow,
            r#"
                SELECT
                  integration_connection.id,
                  integration_connection.user_id,
                  integration_connection.provider_user_id,
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
                  integration_connection.registered_oauth_scopes as "registered_oauth_scopes: Json<Vec<String>>",
                  integration_connection.auto_paused_by_plan_at,
                  integration_connection.auto_paused_config_snapshot as "auto_paused_config_snapshot: Json<IntegrationConnectionConfig>",
                  integration_connection.paused_at,
                  integration_connection.paused_reason
                FROM integration_connection
                INNER JOIN integration_connection_config
                  ON integration_connection.id = integration_connection_config.integration_connection_id
                WHERE
                    integration_connection.context->'content'->>'team_id' = $1
                    AND integration_connection.provider_kind::TEXT = 'Slack'
                    AND ($2::TEXT[] IS NULL OR integration_connection.provider_user_id = ANY($2))
            "#,
            team_id,
            provider_user_ids
        )
            .fetch_all(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!(
                    "Failed to fetch Slack integration connections of team {team_id} from storage: {err}"
                );
                UniversalInboxError::DatabaseError { source: err, message }
            })?;

        rows.iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<IntegrationConnection>, UniversalInboxError>>()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = for_user_id.map(|id| id.to_string()))
    )]
    async fn find_slack_integration_connections_without_context(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        for_user_id: Option<UserId>,
    ) -> Result<Vec<SlackIntegrationConnectionWithoutContext>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
                SELECT
                  integration_connection.id,
                  integration_connection.user_id,
                  integration_connection.status::TEXT AS status,
                  EXISTS (
                    SELECT 1 FROM oauth_credential
                    WHERE oauth_credential.integration_connection_id = integration_connection.id
                  ) AS has_credential
                FROM integration_connection
                WHERE integration_connection.provider_kind::TEXT = 'Slack'
                  AND integration_connection.context IS NULL
            "#,
        );
        if let Some(for_user_id) = for_user_id {
            query_builder
                .push(" AND integration_connection.user_id = ")
                .push_bind(for_user_id.0);
        }
        query_builder.push(" ORDER BY integration_connection.id");

        let rows: Vec<SlackIntegrationConnectionWithoutContextRow> = query_builder
            .build_query_as()
            .fetch_all(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!(
                    "Failed to fetch Slack integration connections without context from storage: {err}"
                );
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        rows.into_iter()
            .map(|row| {
                let status =
                    row.status
                        .parse()
                        .map_err(|e| UniversalInboxError::InvalidEnumData {
                            source: e,
                            output: row.status.clone(),
                        })?;
                Ok(SlackIntegrationConnectionWithoutContext {
                    id: IntegrationConnectionId(row.id),
                    user_id: UserId(row.user_id),
                    status,
                    has_credential: row.has_credential,
                })
            })
            .collect()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::INTEGRATION_CONNECTION_ID } = integration_connection_id.to_string(),
            { attr::INTEGRATION_CONNECTION_STATUS } = new_status.to_string(),
            { attr::USER_ID } = for_user_id.to_string()
        )
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
        // `updated_at` dates the current status: the long-failing pause falls
        // back to it when no sync failure dated the `Failing` status.
        separated
            .push(" updated_at = CASE WHEN integration_connection.status::TEXT != ")
            .push_bind_unseparated(new_status.to_string())
            .push_unseparated(
                " THEN (now() at time zone 'utc') ELSE integration_connection.updated_at END",
            );
        // Leaving `Paused` (disconnect, reconnect) clears the pause marker;
        // only `pause_integration_connection` sets it.
        if new_status != IntegrationConnectionStatus::Paused {
            separated.push(" paused_at = NULL");
            separated.push(" paused_reason = NULL");
        }

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
                  integration_connection.auto_paused_by_plan_at,
                  integration_connection.auto_paused_config_snapshot as auto_paused_config_snapshot,
                  integration_connection.paused_at,
                  integration_connection.paused_reason,
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
            { attr::USER_ID } = user_id.map(|x| x.to_string()),
            { attr::INTEGRATION_PROVIDER_KIND } = integration_provider_kind.map(|x| x.to_string())
        )
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

        // Route the row selection through an ordered, row-locking subquery rather than
        // a bare WHERE on the UPDATE (UPDATE itself has no ORDER BY). When `user_id` or
        // `integration_provider_kind` is None this predicate can match more than one row
        // (e.g. the unauthenticated global sync-trigger endpoint updates every connection
        // for a given provider, or every connection for a given user); without a
        // deterministic lock order, two overlapping callers scanning the same row set in
        // different orders is exactly the shape that produces a Postgres deadlock
        // (SQLSTATE 40P01).
        query_builder
            .push(" FROM integration_connection_config ")
            .push(
                " WHERE integration_connection_config.integration_connection_id = integration_connection.id ",
            )
            .push(" AND integration_connection.id IN ( ")
            .push(" SELECT integration_connection.id FROM integration_connection WHERE TRUE ");
        // Not `.separated(" AND ")`: a fresh `Separated` does not prepend its separator
        // before its *first* push, so the first optional predicate here would otherwise
        // concatenate directly onto "WHERE TRUE" with no connective. Each predicate is
        // independently optional (not a comma/and-joined list built from scratch), so
        // prepend " AND " explicitly instead.
        if let Some(integration_provider_kind) = integration_provider_kind {
            query_builder
                .push(" AND integration_connection.provider_kind::TEXT = ")
                .push_bind(integration_provider_kind.to_string());
        }
        if let Some(user_id) = user_id {
            query_builder
                .push(" AND integration_connection.user_id = ")
                .push_bind(user_id.0);
        }
        query_builder.push(" ORDER BY integration_connection.id FOR UPDATE ) ");

        query_builder.push(
            r#"
                RETURNING
                  integration_connection.id,
                  integration_connection.user_id,
                  integration_connection.provider_user_id,
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
                  integration_connection.auto_paused_by_plan_at,
                  integration_connection.auto_paused_config_snapshot as auto_paused_config_snapshot,
                  integration_connection.paused_at,
                  integration_connection.paused_reason,
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
            { attr::INTEGRATION_CONNECTION_ID } = integration_connection_id.to_string()
        )
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
                  integration_connection.auto_paused_by_plan_at,
                  integration_connection.auto_paused_config_snapshot as auto_paused_config_snapshot,
                  integration_connection.paused_at,
                  integration_connection.paused_reason,
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
            { attr::USER_ID } = for_user_id.to_string(),
            { attr::INTEGRATION_CONNECTION_STATUS } = ?status,
            { attr::INTEGRATION_CONNECTION_LOCK_ROWS } = lock_rows
        )
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
                  integration_connection.registered_oauth_scopes,
                  integration_connection.auto_paused_by_plan_at,
                  integration_connection.auto_paused_config_snapshot as auto_paused_config_snapshot,
                  integration_connection.paused_at,
                  integration_connection.paused_reason
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
        // Deterministic row order so concurrent callers that lock (or update) several rows
        // from this same set always do so in the same order, closing off one class of
        // Postgres deadlock (SQLSTATE 40P01) between two overlapping transactions.
        query_builder.push(" ORDER BY integration_connection.id ");
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
        fields(
            { attr::USER_ID } = for_user_id.to_string(),
            { attr::INTEGRATION_PROVIDER_KINDS } = ?provider_kinds,
        )
    )]
    async fn claim_due_notification_syncs(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        for_user_id: UserId,
        provider_kinds: &[IntegrationProviderKind],
        now: DateTime<Utc>,
        synced_before: DateTime<Utc>,
    ) -> Result<Vec<(IntegrationConnectionId, IntegrationProviderKind)>, UniversalInboxError> {
        claim_due_syncs(
            executor,
            "last_notifications_sync_scheduled_at",
            for_user_id,
            provider_kinds,
            now,
            synced_before,
        )
        .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::USER_ID } = for_user_id.to_string(),
            { attr::INTEGRATION_PROVIDER_KINDS } = ?provider_kinds,
        )
    )]
    async fn claim_due_task_syncs(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        for_user_id: UserId,
        provider_kinds: &[IntegrationProviderKind],
        now: DateTime<Utc>,
        synced_before: DateTime<Utc>,
    ) -> Result<Vec<(IntegrationConnectionId, IntegrationProviderKind)>, UniversalInboxError> {
        claim_due_syncs(
            executor,
            "last_tasks_sync_scheduled_at",
            for_user_id,
            provider_kinds,
            now,
            synced_before,
        )
        .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::INTEGRATION_CONNECTION_ID } = integration_connection_id.to_string())
    )]
    async fn claim_notification_sync_start(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        now: DateTime<Utc>,
        synced_before: Option<DateTime<Utc>>,
    ) -> Result<bool, UniversalInboxError> {
        claim_sync_start(
            executor,
            "last_notifications_sync_started_at",
            integration_connection_id,
            now,
            synced_before,
        )
        .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::INTEGRATION_CONNECTION_ID } = integration_connection_id.to_string())
    )]
    async fn claim_task_sync_start(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        now: DateTime<Utc>,
        synced_before: Option<DateTime<Utc>>,
    ) -> Result<bool, UniversalInboxError> {
        claim_sync_start(
            executor,
            "last_tasks_sync_started_at",
            integration_connection_id,
            now,
            synced_before,
        )
        .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = for_user_id.to_string())
    )]
    async fn count_validated_integration_connections(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        for_user_id: UserId,
    ) -> Result<u32, UniversalInboxError> {
        count_validated_connections(executor, for_user_id, PlanPauseFilter::Active).await
    }

    #[tracing::instrument(level = "debug", skip_all, fields({ attr::USER_ID } = for_user_id.to_string()))]
    async fn count_plan_paused_integration_connections(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        for_user_id: UserId,
    ) -> Result<u32, UniversalInboxError> {
        count_validated_connections(executor, for_user_id, PlanPauseFilter::Paused).await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::INTEGRATION_CONNECTION_ID } = integration_connection.id.to_string())
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
                    $3::integration_provider_kind,
                    $4::integration_connection_status,
                    $5,
                    $6,
                    $7,
                    $8,
                    $9,
                    $10,
                    $11
                  )
            "#,
            integration_connection.id.0,
            integration_connection.user_id.0,
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
        fields({ attr::INTEGRATION_CONNECTION_ID } = id.to_string())
    )]
    async fn does_integration_connection_exist(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: IntegrationConnectionId,
    ) -> Result<bool, UniversalInboxError> {
        let count: Option<i64> = sqlx::query_scalar!(
            r#"SELECT count(*) FROM integration_connection WHERE id = $1"#,
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

        if let Some(1) = count {
            return Ok(true);
        }
        return Ok(false);
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::INTEGRATION_CONNECTION_ID } = integration_connection_id.to_string(),
            { attr::USER_ID } = for_user_id.to_string()
        )
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
                  integration_connection.auto_paused_by_plan_at,
                  integration_connection.auto_paused_config_snapshot as auto_paused_config_snapshot,
                  integration_connection.paused_at,
                  integration_connection.paused_reason,
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
        fields({ attr::INTEGRATION_CONNECTION_ID } = integration_connection_id.to_string())
    )]
    async fn set_integration_connection_plan_pause(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        paused_at: Option<chrono::DateTime<chrono::Utc>>,
        snapshot: Option<&IntegrationConnectionConfig>,
    ) -> Result<(), UniversalInboxError> {
        // Strip the timezone to match the column's `TIMESTAMP` (without time
        // zone) type — every other temporal column on this table is naive UTC.
        let naive = paused_at.map(|dt| dt.naive_utc());
        // Marker and snapshot are written together so the
        // `auto_paused_by_plan_at IS NOT NULL ⟺ auto_paused_config_snapshot IS NOT NULL`
        // invariant can never be observed half-applied. `None` clears both.
        let snapshot = snapshot.map(Json);
        sqlx::query!(
            r#"
            UPDATE integration_connection
            SET auto_paused_by_plan_at = $2,
                auto_paused_config_snapshot = $3,
                updated_at = (now() at time zone 'utc')
            WHERE id = $1
            "#,
            integration_connection_id.0,
            naive,
            snapshot as Option<Json<&IntegrationConnectionConfig>>,
        )
        .execute(&mut **executor)
        .await
        .map_err(|err| UniversalInboxError::DatabaseError {
            message: format!(
                "Failed to set plan-pause state for {integration_connection_id}: {err}"
            ),
            source: err,
        })?;
        Ok(())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::INTEGRATION_CONNECTION_ID } = integration_connection_id.to_string()
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
                  integration_connection.auto_paused_by_plan_at,
                  integration_connection.auto_paused_config_snapshot as auto_paused_config_snapshot,
                  integration_connection.paused_at,
                  integration_connection.paused_reason,
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

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::INTEGRATION_CONNECTION_INACTIVE_BEFORE } = inactive_before.to_rfc3339()
        )
    )]
    async fn find_validated_integration_connections_of_inactive_users(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        provider_kinds: &[IntegrationProviderKind],
        inactive_before: DateTime<Utc>,
        warned_before: Option<DateTime<Utc>>,
    ) -> Result<Vec<(IntegrationConnectionId, UserId)>, UniversalInboxError> {
        // A warning sent before the user's last activity belongs to a previous
        // inactivity period and does not count.
        let provider_kind_names: Vec<String> = provider_kinds
            .iter()
            .map(|provider_kind| provider_kind.to_string())
            .collect();
        let rows = sqlx::query!(
            r#"
                SELECT integration_connection.id, integration_connection.user_id
                FROM integration_connection
                INNER JOIN "user" ON "user".id = integration_connection.user_id
                WHERE integration_connection.provider_kind::TEXT = ANY($1)
                  AND integration_connection.status = 'Validated'
                  AND "user".last_active_at < $2
                  AND (
                    $3::TIMESTAMP IS NULL
                    OR (
                      integration_connection.inactivity_warning_sent_at >= "user".last_active_at
                      AND integration_connection.inactivity_warning_sent_at < $3
                    )
                  )
                ORDER BY "user".last_active_at
            "#,
            &provider_kind_names,
            inactive_before.naive_utc(),
            warned_before.map(|warned_before| warned_before.naive_utc()) as Option<NaiveDateTime>,
        )
        .fetch_all(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!(
                "Failed to list {provider_kinds:?} integration connections of inactive users: {err}"
            );
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        Ok(rows
            .into_iter()
            .map(|row| (IntegrationConnectionId(row.id), UserId(row.user_id)))
            .collect())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::INTEGRATION_CONNECTION_INACTIVE_BEFORE } = inactive_before.to_rfc3339()
        )
    )]
    async fn find_integration_connections_to_warn_of_inactivity(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        provider_kinds: &[IntegrationProviderKind],
        inactive_before: DateTime<Utc>,
    ) -> Result<Vec<(IntegrationConnectionId, UserId)>, UniversalInboxError> {
        let provider_kind_names: Vec<String> = provider_kinds
            .iter()
            .map(|provider_kind| provider_kind.to_string())
            .collect();
        let rows = sqlx::query!(
            r#"
                SELECT integration_connection.id, integration_connection.user_id
                FROM integration_connection
                INNER JOIN "user" ON "user".id = integration_connection.user_id
                WHERE integration_connection.provider_kind::TEXT = ANY($1)
                  AND integration_connection.status = 'Validated'
                  AND "user".last_active_at < $2
                  AND (
                    integration_connection.inactivity_warning_sent_at IS NULL
                    OR integration_connection.inactivity_warning_sent_at < "user".last_active_at
                  )
                ORDER BY "user".last_active_at
            "#,
            &provider_kind_names,
            inactive_before.naive_utc(),
        )
        .fetch_all(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!(
                "Failed to list {provider_kinds:?} integration connections to warn of inactivity: {err}"
            );
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        Ok(rows
            .into_iter()
            .map(|row| (IntegrationConnectionId(row.id), UserId(row.user_id)))
            .collect())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::INTEGRATION_CONNECTION_ID } = integration_connection_id.to_string(),
            { attr::INTEGRATION_CONNECTION_INACTIVE_BEFORE } = inactive_before.to_rfc3339()
        )
    )]
    async fn mark_inactivity_warning_sent(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        inactive_before: DateTime<Utc>,
        sent_at: DateTime<Utc>,
    ) -> Result<bool, UniversalInboxError> {
        let result = sqlx::query!(
            r#"
                UPDATE integration_connection
                SET inactivity_warning_sent_at = $3
                FROM "user"
                WHERE integration_connection.id = $1
                  AND "user".id = integration_connection.user_id
                  AND integration_connection.status = 'Validated'
                  AND "user".last_active_at < $2
                  AND (
                    integration_connection.inactivity_warning_sent_at IS NULL
                    OR integration_connection.inactivity_warning_sent_at < "user".last_active_at
                  )
            "#,
            integration_connection_id.0,
            inactive_before.naive_utc(),
            sent_at.naive_utc(),
        )
        .execute(&mut **executor)
        .await
        .map_err(|err| UniversalInboxError::DatabaseError {
            message: format!(
                "Failed to mark the inactivity warning of integration connection {integration_connection_id} as sent: {err}"
            ),
            source: err,
        })?;

        Ok(result.rows_affected() == 1)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::INTEGRATION_CONNECTION_FAILING_BEFORE } = failing_before.to_rfc3339()
        )
    )]
    async fn find_long_failing_integration_connections(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        provider_kinds: &[IntegrationProviderKind],
        failing_before: DateTime<Utc>,
    ) -> Result<Vec<(IntegrationConnectionId, UserId)>, UniversalInboxError> {
        let provider_kind_names: Vec<String> = provider_kinds
            .iter()
            .map(|provider_kind| provider_kind.to_string())
            .collect();
        let rows = sqlx::query!(
            r#"
                SELECT id, user_id
                FROM integration_connection
                WHERE provider_kind::TEXT = ANY($1)
                  AND status = 'Failing'
                  AND COALESCE(
                    LEAST(first_notifications_sync_failed_at, first_tasks_sync_failed_at),
                    updated_at
                  ) < $2
            "#,
            &provider_kind_names,
            failing_before.naive_utc(),
        )
        .fetch_all(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!(
                "Failed to list long failing {provider_kinds:?} integration connections: {err}"
            );
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        Ok(rows
            .into_iter()
            .map(|row| (IntegrationConnectionId(row.id), UserId(row.user_id)))
            .collect())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::INTEGRATION_CONNECTION_ID } = integration_connection_id.to_string(),
            { attr::INTEGRATION_CONNECTION_PAUSED_REASON } = paused_reason.to_string()
        )
    )]
    async fn pause_integration_connection(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        paused_at: DateTime<Utc>,
        paused_reason: IntegrationConnectionPausedReason,
    ) -> Result<Option<IntegrationConnection>, UniversalInboxError> {
        sqlx::query!(
            r#"
                UPDATE integration_connection
                SET status = 'Paused',
                    failure_message = NULL,
                    paused_at = $2,
                    paused_reason = $3,
                    inactivity_warning_sent_at = NULL,
                    updated_at = (now() at time zone 'utc')
                WHERE id = $1
            "#,
            integration_connection_id.0,
            paused_at.naive_utc(),
            paused_reason.to_string(),
        )
        .execute(&mut **executor)
        .await
        .map_err(|err| UniversalInboxError::DatabaseError {
            message: format!(
                "Failed to pause integration connection {integration_connection_id}: {err}"
            ),
            source: err,
        })?;

        self.get_integration_connection(executor, integration_connection_id)
            .await
    }
}

#[derive(sqlx::Type, Debug)]
#[sqlx(type_name = "integration_connection_status")]
enum PgIntegrationConnectionStatus {
    Created,
    Validated,
    Failing,
    Paused,
}

#[derive(Debug, sqlx::FromRow)]
pub struct IntegrationConnectionRow {
    id: Uuid,
    user_id: Uuid,
    provider_user_id: Option<String>,
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
    auto_paused_by_plan_at: Option<NaiveDateTime>,
    auto_paused_config_snapshot: Option<Json<IntegrationConnectionConfig>>,
    paused_at: Option<NaiveDateTime>,
    paused_reason: Option<String>,
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
            auto_paused_by_plan_at: row
                .auto_paused_by_plan_at
                .map(|t| DateTime::from_naive_utc_and_offset(t, Utc)),
            auto_paused_config_snapshot: row
                .auto_paused_config_snapshot
                .as_ref()
                .map(|snapshot| snapshot.0.clone()),
            paused_at: row
                .paused_at
                .map(|t| DateTime::from_naive_utc_and_offset(t, Utc)),
            paused_reason: row
                .paused_reason
                .as_ref()
                .map(|reason| {
                    reason
                        .parse()
                        .map_err(|e| UniversalInboxError::InvalidEnumData {
                            source: e,
                            output: reason.clone(),
                        })
                })
                .transpose()?,
        })
    }
}

/// Which side of the plan-pause split [`count_validated_connections`] counts.
#[derive(Clone, Copy)]
enum PlanPauseFilter {
    /// Connections holding a Free-plan slot: validated and not plan-paused.
    Active,
    /// Connections the plan switched off; they hold no slot and come back on
    /// upgrade.
    Paused,
}

/// Count a user's `Validated` connections on one side of the plan-pause split,
/// without materialising any row or JSON config.
///
/// Excludes the `API` provider kind: that connection is auto-created on first
/// programmatic API use (`get_or_create_integration_connection`), is never
/// listed in `settings.integrations`, and so never appears in the front-end
/// integrations panel. Counting it would inflate billing usage by one and
/// silently eat a slot of the Free-plan cap. This is the single source of truth
/// for usage display and the connect-time cap check, so the two stay in
/// lockstep.
async fn count_validated_connections(
    executor: &mut Transaction<'_, Postgres>,
    for_user_id: UserId,
    filter: PlanPauseFilter,
) -> Result<u32, UniversalInboxError> {
    let mut query_builder = QueryBuilder::new(
        r#"
            SELECT COUNT(*)
            FROM integration_connection
            INNER JOIN integration_connection_config
              ON integration_connection.id = integration_connection_config.integration_connection_id
            WHERE integration_connection.user_id = "#,
    );
    query_builder.push_bind(for_user_id.0);
    query_builder
        .push(" AND integration_connection.status::TEXT = ")
        .push_bind(IntegrationConnectionStatus::Validated.to_string());
    query_builder
        .push(" AND integration_connection.provider_kind::TEXT != ")
        .push_bind(IntegrationProviderKind::API.to_string());
    query_builder.push(match filter {
        PlanPauseFilter::Active => " AND integration_connection.auto_paused_by_plan_at IS NULL",
        PlanPauseFilter::Paused => " AND integration_connection.auto_paused_by_plan_at IS NOT NULL",
    });

    let count: i64 = query_builder
        .build_query_scalar::<i64>()
        .fetch_one(&mut **executor)
        .await
        .map_err(|err| {
            let message =
                format!("Failed to count integration connections for user {for_user_id}: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

    Ok(count.max(0) as u32)
}
