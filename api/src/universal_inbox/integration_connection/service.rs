use core::fmt;
use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Context, anyhow};
use apalis::prelude::*;
use apalis_redis::RedisStorage;
use cached::proc_macro::concurrent_cached;
use chrono::{DateTime, TimeDelta, Utc};
use clap::ValueEnum;
use oauth2::{CsrfToken, PkceCodeChallenge};
use redis::AsyncCommands;
use secrecy::{ExposeSecret, SecretBox};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use tokio_retry::{
    Retry,
    strategy::{ExponentialBackoff, jitter},
};
use tracing::{debug, error, info, warn};
use url::Url;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionId, IntegrationConnectionPausedReason,
        IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        provider::{IntegrationConnectionContext, IntegrationProviderKind},
    },
    notification::NotificationSyncSourceKind,
    task::TaskSyncSourceKind,
    user::UserId,
};

use crate::{
    integrations::oauth2::{
        AccessToken, AuthorizationCode, PkceVerifier, RefreshToken,
        provider::{OAuth2FlowService, OAuth2Provider, TokenRevocationError},
    },
    jobs::{
        UniversalInboxJob,
        sync::{SyncNotificationsJob, SyncTasksJob},
    },
    repository::{
        Repository,
        integration_connection::{
            IntegrationConnectionRepository, IntegrationConnectionSyncStatusUpdate,
            IntegrationConnectionSyncedBeforeFilter, OAUTH_INVALID_GRANT_ERROR_MESSAGE,
            OAUTH_MISSING_REFRESH_TOKEN_ERROR_MESSAGE, SLACK_ACCESS_REVOKED_ERROR_MESSAGE,
            SlackIntegrationConnectionWithoutContext,
        },
        notification::NotificationRepository,
        oauth_credential::OAuthCredentialRepository,
        oauth_grant_revocation::{
            NewOAuthGrantRevocation, OAuthGrantRevocationRepository, PendingOAuthGrantRevocation,
        },
    },
    universal_inbox::{UniversalInboxError, UpdateStatus, retry_on_transient_database_error},
    utils::{
        cache::{Cache, build_redis_cache},
        crypto::{TokenEncryptionKey, decrypt_token, encrypt_token},
    },
};

/// Exponential backoff of the `retry-oauth-grant-revocations` cron.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrantRevocationRetryPolicy {
    pub base_delay_in_seconds: u64,
    pub max_delay_in_seconds: u64,
    /// After this many failed attempts, the revocation is abandoned.
    pub max_attempts: u32,
}

impl GrantRevocationRetryPolicy {
    /// When to retry after `attempts` failed attempts, `None` once they are
    /// exhausted.
    pub fn next_attempt_at(&self, attempts: u32) -> Option<DateTime<Utc>> {
        if attempts >= self.max_attempts {
            return None;
        }
        let delay_in_seconds = self
            .base_delay_in_seconds
            .saturating_mul(2u64.saturating_pow(attempts.saturating_sub(1)))
            .min(self.max_delay_in_seconds);
        Some(Utc::now() + TimeDelta::seconds(delay_in_seconds.try_into().unwrap_or(i64::MAX)))
    }
}

struct GrantTokens {
    access_token: AccessToken,
    refresh_token: Option<RefreshToken>,
    access_token_expires_at: Option<DateTime<Utc>>,
}

enum GrantRevocationAttempt {
    Revoked,
    /// The provider no longer accepts the token: nothing left to revoke.
    AlreadyDead(String),
    /// Retry later with these tokens (a refresh may have rotated them).
    Failed {
        tokens: GrantTokens,
        error: String,
    },
}

/// What to store after a failed retry.
struct FailedGrantRevocation {
    encrypted_access_token: Vec<u8>,
    encrypted_refresh_token: Option<Vec<u8>>,
    access_token_expires_at: Option<DateTime<Utc>>,
    error: String,
    /// Retrying cannot succeed: abandon the revocation now.
    abandon: bool,
}

enum RefreshFailure {
    Dead(String),
    Failed(String),
}

impl RefreshFailure {
    fn with_tokens(self, tokens: GrantTokens) -> GrantRevocationAttempt {
        match self {
            Self::Dead(detail) => GrantRevocationAttempt::AlreadyDead(detail),
            Self::Failed(error) => GrantRevocationAttempt::Failed { tokens, error },
        }
    }
}

const OAUTH_STATE_PREFIX: &str = "universal-inbox::oauth-state::";
const OAUTH_STATE_TTL_SECONDS: u64 = 600;

#[derive(Debug, Serialize, Deserialize)]
struct OAuthStateData {
    integration_connection_id: IntegrationConnectionId,
    pkce_verifier: Option<SecretBox<PkceVerifier>>,
    provider_kind: IntegrationProviderKind,
    /// User who started the flow. The callback is only honoured for this
    /// user's session, so an authorization response obtained from someone
    /// else (login CSRF / account linking) cannot land in this connection.
    /// Optional only so a state stored before this field existed still
    /// deserializes; such a state is rejected.
    #[serde(default)]
    user_id: Option<UserId>,
}

pub struct IntegrationConnectionService {
    repository: Arc<Repository>,
    required_oauth_scopes: HashMap<IntegrationProviderKind, Vec<String>>,
    oauth2_providers: HashMap<IntegrationProviderKind, Arc<dyn OAuth2Provider>>,
    oauth2_flow_service: OAuth2FlowService,
    token_encryption_key: SecretBox<TokenEncryptionKey>,
    min_sync_notifications_interval_in_minutes: i64,
    min_sync_tasks_interval_in_minutes: i64,
    sync_backoff_base_delay_in_seconds: u64,
    sync_backoff_max_delay_in_seconds: u64,
    sync_failure_window_in_hours: i64,
    /// Optional plug-in for plan-based limit checks. `None` on instances
    /// without `[billing]` configured (self-hosted): all calls short-circuit
    /// to the unlimited behaviour Universal Inbox has always had.
    billing_service: Option<Arc<crate::billing::service::BillingService>>,
}

#[derive(Debug)]
pub enum IntegrationConnectionSyncType {
    Notifications,
    Tasks,
}

impl fmt::Display for IntegrationConnectionSyncType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            IntegrationConnectionSyncType::Notifications => write!(f, "Notifications"),
            IntegrationConnectionSyncType::Tasks => write!(f, "Tasks"),
        }
    }
}

impl IntegrationConnectionService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repository: Arc<Repository>,
        required_oauth_scopes: HashMap<IntegrationProviderKind, Vec<String>>,
        oauth2_providers: HashMap<IntegrationProviderKind, Arc<dyn OAuth2Provider>>,
        oauth2_flow_service: OAuth2FlowService,
        token_encryption_key: SecretBox<TokenEncryptionKey>,
        min_sync_notifications_interval_in_minutes: i64,
        min_sync_tasks_interval_in_minutes: i64,
        sync_backoff_base_delay_in_seconds: u64,
        sync_backoff_max_delay_in_seconds: u64,
        sync_failure_window_in_hours: i64,
        // Plan-based enforcement (Free integration cap). `None` on instances
        // without `[billing]` configured. Required at construction so "billing
        // is wired" is a type-enforced invariant, not a post-hoc mutation.
        billing_service: Option<Arc<crate::billing::service::BillingService>>,
    ) -> IntegrationConnectionService {
        IntegrationConnectionService {
            repository,
            required_oauth_scopes,
            oauth2_providers,
            oauth2_flow_service,
            token_encryption_key,
            min_sync_notifications_interval_in_minutes,
            min_sync_tasks_interval_in_minutes,
            sync_backoff_base_delay_in_seconds,
            sync_backoff_max_delay_in_seconds,
            sync_failure_window_in_hours,
            billing_service,
        }
    }

    /// Count *validated* integration connections for a user. Used by the
    /// billing subsystem (via the [`IntegrationConnectionCounter`] adapter)
    /// to enforce the Free-plan cap.
    pub async fn count_validated_integration_connections(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        for_user_id: UserId,
    ) -> Result<u32, UniversalInboxError> {
        self.repository
            .count_validated_integration_connections(executor, for_user_id)
            .await
    }

    pub async fn begin(&self) -> Result<Transaction<'_, Postgres>, UniversalInboxError> {
        self.repository.begin().await
    }

    /// Returns a clone of the underlying database connection pool.
    ///
    /// Used by `/ping` so the health check can probe the pool with a
    /// short-lived `SELECT 1` without acquiring a long-lived
    /// `Transaction<'_, Postgres>`. Holding a real transaction per `/ping`
    /// call lets an unauthenticated attacker pin every connection in the
    /// pool — a DoS amplifier on a public endpoint.
    pub fn pool(&self) -> Arc<sqlx::PgPool> {
        self.repository.pool.clone()
    }

    fn get_oauth2_provider(&self, kind: &IntegrationProviderKind) -> Option<&dyn OAuth2Provider> {
        self.oauth2_providers.get(kind).map(|p| p.as_ref())
    }

    pub async fn get_integration_connection(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
    ) -> Result<Option<IntegrationConnection>, UniversalInboxError> {
        self.repository
            .get_integration_connection(executor, integration_connection_id)
            .await
    }

    pub async fn fetch_all_integration_connections(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        for_user_id: UserId,
        status: Option<IntegrationConnectionStatus>,
        lock_rows: bool,
    ) -> Result<Vec<IntegrationConnection>, UniversalInboxError> {
        self.repository
            .fetch_all_integration_connections(executor, for_user_id, status, lock_rows)
            .await
    }

    /// Atomically claims every one of `for_user_id`'s connections whose notifications or
    /// tasks sync is due, and pushes one job per claimed connection.
    ///
    /// Replaces the old `trigger_sync_for_integration_connections` (a Rust-side
    /// read-then-per-connection-write loop, run on the caller's own transaction): the claim
    /// is one atomic, `FOR UPDATE SKIP LOCKED` statement per sync type in a *short*
    /// transaction this method owns and commits itself, so callers should invoke it after
    /// their own transaction (e.g. the one that fetched a notifications/tasks page) has
    /// already committed — this can then never hold up, or be deadlocked by, that read.
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = for_user_id.to_string()),
        err
    )]
    pub async fn schedule_due_syncs(
        &self,
        for_user_id: UserId,
        job_storage: &mut RedisStorage<UniversalInboxJob>,
    ) -> Result<(), UniversalInboxError> {
        let now = Utc::now();
        let notifications_synced_before = now
            - TimeDelta::try_minutes(self.min_sync_notifications_interval_in_minutes)
                .unwrap_or_else(|| {
                    panic!(
                        "Invalid `min_sync_notifications_interval_in_minutes` value: {}",
                        self.min_sync_notifications_interval_in_minutes
                    )
                });
        let tasks_synced_before = now
            - TimeDelta::try_minutes(self.min_sync_tasks_interval_in_minutes).unwrap_or_else(
                || {
                    panic!(
                        "Invalid `min_sync_tasks_interval_in_minutes` value: {}",
                        self.min_sync_tasks_interval_in_minutes
                    )
                },
            );

        // tag: New notification integration
        let notification_provider_kinds: Vec<IntegrationProviderKind> =
            NotificationSyncSourceKind::value_variants()
                .iter()
                .map(|kind| (*kind).into())
                .collect();
        let task_provider_kinds: Vec<IntegrationProviderKind> =
            TaskSyncSourceKind::value_variants()
                .iter()
                .map(|kind| (*kind).into())
                .collect();

        // Retried on a transient DB error (deadlock/serialization/lock-timeout): this
        // whole block is one short, idempotent claim — safe to just redo it.
        let (claimed_notification_syncs, claimed_task_syncs) =
            retry_on_transient_database_error(|| async {
                let mut transaction = self
                    .begin()
                    .await
                    .context("Failed to create new transaction while claiming due syncs")?;
                let claimed_notification_syncs = self
                    .repository
                    .claim_due_notification_syncs(
                        &mut transaction,
                        for_user_id,
                        &notification_provider_kinds,
                        now,
                        notifications_synced_before,
                    )
                    .await?;
                let claimed_task_syncs = self
                    .repository
                    .claim_due_task_syncs(
                        &mut transaction,
                        for_user_id,
                        &task_provider_kinds,
                        now,
                        tasks_synced_before,
                    )
                    .await?;
                transaction
                    .commit()
                    .await
                    .context("Failed to commit while claiming due syncs")?;
                Ok((claimed_notification_syncs, claimed_task_syncs))
            })
            .await?;

        for (_integration_connection_id, provider_kind) in claimed_notification_syncs {
            if let Ok(notification_sync_source_kind) =
                NotificationSyncSourceKind::try_from(provider_kind)
            {
                self.push_sync_notifications_job(
                    job_storage,
                    Some(notification_sync_source_kind),
                    Some(for_user_id),
                )
                .await?;
            }
        }
        for (_integration_connection_id, provider_kind) in claimed_task_syncs {
            if let Ok(task_sync_source_kind) = TaskSyncSourceKind::try_from(provider_kind) {
                self.push_sync_tasks_job(
                    job_storage,
                    Some(task_sync_source_kind),
                    Some(for_user_id),
                )
                .await?;
            }
        }

        Ok(())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            notification_sync_source_kind = notification_sync_source_kind.map(|kind| kind.to_string()),
            user.id = for_user_id.map(|id| id.to_string())
        ),
        err
    )]
    pub async fn trigger_sync_notifications(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        notification_sync_source_kind: Option<NotificationSyncSourceKind>,
        for_user_id: Option<UserId>,
        job_storage: &mut RedisStorage<UniversalInboxJob>,
    ) -> Result<(), UniversalInboxError> {
        info!(
            "Triggering sync notifications job for {notification_sync_source_kind:?} integration connection for user {for_user_id:?}"
        );
        self.schedule_notifications_sync_status(
            executor,
            notification_sync_source_kind.map(|kind| kind.into()),
            for_user_id,
        )
        .await?;

        Retry::start(
            ExponentialBackoff::from_millis(10).map(jitter).take(10),
            || async {
                job_storage
                    .clone()
                    .push(UniversalInboxJob::SyncNotifications(SyncNotificationsJob {
                        source: notification_sync_source_kind,
                        user_id: for_user_id,
                    }))
                    .await
            },
        )
        .await
        .context("Failed to push SyncNotifications job to queue")?;

        Ok(())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            task_sync_source_kind = task_sync_source_kind.map(|kind| kind.to_string()),
            user.id = for_user_id.map(|id| id.to_string())
        ),
        err
    )]
    pub async fn trigger_sync_tasks(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        task_sync_source_kind: Option<TaskSyncSourceKind>,
        for_user_id: Option<UserId>,
        job_storage: &mut RedisStorage<UniversalInboxJob>,
    ) -> Result<(), UniversalInboxError> {
        info!(
            "Triggering sync tasks job for {task_sync_source_kind:?} integration connection for user {for_user_id:?}"
        );
        self.schedule_tasks_sync_status(
            executor,
            task_sync_source_kind.map(|kind| kind.into()),
            for_user_id,
        )
        .await?;

        Retry::start(
            ExponentialBackoff::from_millis(10).map(jitter).take(10),
            || async {
                job_storage
                    .clone()
                    .push(UniversalInboxJob::SyncTasks(SyncTasksJob {
                        source: task_sync_source_kind,
                        user_id: for_user_id,
                    }))
                    .await
            },
        )
        .await
        .context("Failed to push SyncTasks job to queue")?;

        Ok(())
    }

    /// Pushes a `SyncNotifications` job to the queue without touching the database.
    ///
    /// Split out of `trigger_sync_notifications` so HTTP handlers that schedule a sync
    /// outside a caller-visible batch (the sync-trigger endpoint's authenticated and
    /// unauthenticated branches) can commit their DB transaction first and only then push
    /// the job — the Redis push retries with backoff (`tokio-retry`, up to 10 attempts) and
    /// must never hold a Postgres row lock open for that long.
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            notification_sync_source_kind = notification_sync_source_kind.map(|kind| kind.to_string()),
            user.id = for_user_id.map(|id| id.to_string())
        ),
        err
    )]
    pub async fn push_sync_notifications_job(
        &self,
        job_storage: &mut RedisStorage<UniversalInboxJob>,
        notification_sync_source_kind: Option<NotificationSyncSourceKind>,
        for_user_id: Option<UserId>,
    ) -> Result<(), UniversalInboxError> {
        Retry::start(
            ExponentialBackoff::from_millis(10).map(jitter).take(10),
            || async {
                job_storage
                    .clone()
                    .push(UniversalInboxJob::SyncNotifications(SyncNotificationsJob {
                        source: notification_sync_source_kind,
                        user_id: for_user_id,
                    }))
                    .await
            },
        )
        .await
        .context("Failed to push SyncNotifications job to queue")?;

        Ok(())
    }

    /// Pushes a `SyncTasks` job to the queue without touching the database. See
    /// [`Self::push_sync_notifications_job`] for why this is split out of
    /// `trigger_sync_tasks`.
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            task_sync_source_kind = task_sync_source_kind.map(|kind| kind.to_string()),
            user.id = for_user_id.map(|id| id.to_string())
        ),
        err
    )]
    pub async fn push_sync_tasks_job(
        &self,
        job_storage: &mut RedisStorage<UniversalInboxJob>,
        task_sync_source_kind: Option<TaskSyncSourceKind>,
        for_user_id: Option<UserId>,
    ) -> Result<(), UniversalInboxError> {
        Retry::start(
            ExponentialBackoff::from_millis(10).map(jitter).take(10),
            || async {
                job_storage
                    .clone()
                    .push(UniversalInboxJob::SyncTasks(SyncTasksJob {
                        source: task_sync_source_kind,
                        user_id: for_user_id,
                    }))
                    .await
            },
        )
        .await
        .context("Failed to push SyncTasks job to queue")?;

        Ok(())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            user.id = for_user_id.to_string(),
            integration_provider_kind = integration_provider_kind.to_string(),
            status = status.to_string(),
        ),
        err
    )]
    pub async fn create_integration_connection(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_provider_kind: IntegrationProviderKind,
        status: IntegrationConnectionStatus,
        for_user_id: UserId,
    ) -> Result<Box<IntegrationConnection>, UniversalInboxError> {
        // Free-plan integration cap. No-op when [billing] is not configured
        // (self-hosted) or when the user is on Paid; returns HTTP 402 with
        // a structured code that the UI uses to surface the upgrade modal.
        if let Some(billing) = &self.billing_service {
            billing
                .assert_can_add_integration(executor, for_user_id, integration_provider_kind)
                .await?;
        }

        let integration_connection = Box::new(IntegrationConnection::new(
            for_user_id,
            integration_provider_kind.default_integration_connection_config(),
            status,
        ));

        self.repository
            .create_integration_connection(executor, integration_connection)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            user.id = for_user_id.to_string(),
            integration_provider_kind = integration_provider_kind.to_string()
        ),
        err
    )]
    pub async fn get_or_create_integration_connection(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_provider_kind: IntegrationProviderKind,
        for_user_id: UserId,
    ) -> Result<Box<IntegrationConnection>, UniversalInboxError> {
        if let Some(integration_connection) = self
            .repository
            .get_integration_connection_per_provider(
                executor,
                for_user_id,
                integration_provider_kind,
                None,
                Some(IntegrationConnectionStatus::Validated),
            )
            .await?
        {
            return Ok(Box::new(integration_connection));
        }
        self.create_integration_connection(
            executor,
            integration_provider_kind,
            IntegrationConnectionStatus::Validated,
            for_user_id,
        )
        .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_connection_id = integration_connection_id.to_string(),
            user.id = for_user_id.to_string()
        ),
        err
    )]
    pub async fn update_integration_connection_config(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        integration_connection_config: IntegrationConnectionConfig,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<IntegrationConnectionConfig>>, UniversalInboxError> {
        // Updating a configuration writes the configuration and nothing else: no
        // notification is removed and no notification loses its status,
        // `last_read_at`, `snoozed_until` or `task_id`. Reconciling the inbox with
        // the new configuration is the following sync's job.

        // A plan-paused connection must not be re-enabled through a config
        // PATCH: refuse with 402 so the upgrade modal fires, as the
        // integration-cap path does. Edits that keep syncs off stay allowed.
        // `disable_all_syncs()` reports whether it had anything left to switch
        // off, so every toggle the pause covers is covered here too.
        if let Some(connection) = self
            .repository
            .get_integration_connection(executor, integration_connection_id)
            .await?
            && connection.user_id == for_user_id
            && connection.auto_paused_by_plan_at.is_some()
        {
            let enables_sync =
                universal_inbox::integration_connection::provider::IntegrationProvider::new(
                    integration_connection_config.clone(),
                    None,
                )
                .map(|mut provider| provider.disable_all_syncs())
                .unwrap_or(false);
            if enables_sync {
                return Err(UniversalInboxError::PaymentRequired {
                    code: "free_plan_connection_paused",
                    message: "This integration is paused because your Free plan is over its \
                              connection limit. Upgrade to re-enable it."
                        .to_string(),
                    details: serde_json::json!({ "current_plan": "free" }),
                });
            }
        }

        let updated_integration_connection_config = self
            .repository
            .update_integration_connection_config(
                executor,
                integration_connection_id,
                integration_connection_config,
                for_user_id,
            )
            .await?;

        if updated_integration_connection_config
            == (UpdateStatus {
                updated: false,
                result: None,
            })
            && self
                .repository
                .does_integration_connection_exist(executor, integration_connection_id)
                .await?
        {
            return Err(UniversalInboxError::Forbidden(format!(
                "Only the owner of the integration connection {integration_connection_id} can patch it"
            )));
        }

        // Muting an integration takes its notifications out of the inbox right
        // away, so the screen matches what the user just asked for. Bringing them
        // back waits for the sync that reconciles them.
        if let Some(integration_connection) = self
            .repository
            .get_integration_connection(executor, integration_connection_id)
            .await?
        {
            self.reconcile_set_aside_notifications(executor, &integration_connection)
                .await?;
        }

        Ok(updated_integration_connection_config)
    }

    /// Set aside the notifications of an integration that stopped feeding the
    /// inbox because of something the user did. Declarative rather than driven by
    /// transitions: a connection that still feeds the inbox — including one the
    /// API moved to `Failing` on its own — computes `false` and writes nothing.
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_connection_id = integration_connection.id.to_string(),
            user.id = integration_connection.user_id.to_string()
        ),
        err
    )]
    pub async fn reconcile_set_aside_notifications(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection: &IntegrationConnection,
    ) -> Result<u64, UniversalInboxError> {
        if !integration_connection.should_set_aside_notifications() {
            return Ok(0);
        }
        let Some(notification_source_kind) = integration_connection
            .provider
            .config()
            .notification_source_kind()
        else {
            return Ok(0);
        };

        self.repository
            .set_aside_notifications(
                executor,
                notification_source_kind,
                integration_connection.user_id,
            )
            .await
    }

    /// Bring back the notifications of an integration that feeds the inbox
    /// again. Called on a *successful* sync only, so what comes back is a
    /// reconciled inbox rather than an archive: the stale pass ignores
    /// `set_aside_at`, so anything that vanished while the notifications were set
    /// aside has already been retired by the time they are restored.
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_connection_id = integration_connection.id.to_string(),
            user.id = integration_connection.user_id.to_string()
        ),
        err
    )]
    pub async fn restore_set_aside_notifications(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection: &IntegrationConnection,
    ) -> Result<u64, UniversalInboxError> {
        if integration_connection.should_set_aside_notifications() {
            return Ok(0);
        }
        let Some(notification_source_kind) = integration_connection
            .provider
            .config()
            .notification_source_kind()
        else {
            return Ok(0);
        };

        self.repository
            .restore_set_aside_notifications(
                executor,
                notification_source_kind,
                integration_connection.user_id,
            )
            .await
    }

    /// Null out every sync timestamp on the given integration connection so the
    /// UI's sync_summary falls through to the "pending" arm (renders the
    /// `.sync-led.pending` LED). Used only by the documentation screenshot
    /// generator.
    #[cfg(feature = "screenshots")]
    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn force_clear_sync_state(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
    ) -> Result<(), UniversalInboxError> {
        sqlx::query(
            r#"
                UPDATE integration_connection
                SET last_notifications_sync_scheduled_at = NULL,
                    last_notifications_sync_started_at = NULL,
                    last_notifications_sync_completed_at = NULL,
                    last_notifications_sync_failed_at = NULL,
                    last_notifications_sync_failure_message = NULL,
                    notifications_sync_failures = 0,
                    first_notifications_sync_failed_at = NULL,
                    last_tasks_sync_scheduled_at = NULL,
                    last_tasks_sync_started_at = NULL,
                    last_tasks_sync_completed_at = NULL,
                    last_tasks_sync_failed_at = NULL,
                    last_tasks_sync_failure_message = NULL,
                    tasks_sync_failures = 0,
                    first_tasks_sync_failed_at = NULL
                WHERE id = $1
            "#,
        )
        .bind(integration_connection_id.0)
        .execute(&mut **executor)
        .await
        .map_err(|err| UniversalInboxError::DatabaseError {
            source: err,
            message: format!(
                "Failed to clear sync state on integration connection {integration_connection_id}"
            ),
        })?;
        Ok(())
    }

    /// Force-set an integration connection's status and registered OAuth scopes.
    ///
    /// Only used by the documentation screenshot generator (`cargo run --features
    /// screenshots -- test generate-doc-screenshots`) to reproduce error/edge-state
    /// UIs against a throwaway test user. Not exposed via HTTP routes.
    #[cfg(feature = "screenshots")]
    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn force_set_integration_connection_state(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        status: IntegrationConnectionStatus,
        failure_message: Option<String>,
        registered_oauth_scopes: Option<Vec<String>>,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError> {
        self.repository
            .update_integration_connection_status(
                executor,
                integration_connection_id,
                status,
                failure_message,
                registered_oauth_scopes,
                for_user_id,
            )
            .await
    }

    /// Revoke, at the provider, the OAuth grant behind an integration
    /// connection, using its stored credential, before that credential is
    /// deleted. When the provider does not accept the revocation, the tokens
    /// are queued in `oauth_grant_revocation` and retried by the
    /// `retry-oauth-grant-revocations` cron, so a token is never lost before
    /// it is revoked. Only a credential that cannot be decrypted is dropped:
    /// nothing could revoke it.
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_connection_id = integration_connection.id.to_string(),
            provider_kind = integration_connection.provider.kind().to_string()
        ),
        err
    )]
    pub async fn revoke_provider_grant(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection: &IntegrationConnection,
    ) -> Result<(), UniversalInboxError> {
        let provider_kind = integration_connection.provider.kind();
        let Some(provider) = self.get_oauth2_provider(&provider_kind) else {
            return Ok(());
        };
        // The lock keeps the refresh cron from rotating the tokens under us.
        let Some(credential) = self
            .repository
            .lock_oauth_credential(executor, integration_connection.id)
            .await?
        else {
            return Ok(());
        };

        let tokens = match self.decrypt_grant_tokens(
            &credential.encrypted_access_token,
            credential.encrypted_refresh_token.as_deref(),
            credential.access_token_expires_at,
            integration_connection.id.0.as_bytes(),
        ) {
            Ok(tokens) => tokens,
            Err(err) => {
                warn!(
                    "Failed to decrypt the OAuth credential of integration connection {} to revoke it: {err:?}",
                    integration_connection.id
                );
                return Ok(());
            }
        };

        match self.attempt_grant_revocation(provider, tokens).await {
            GrantRevocationAttempt::Revoked => info!(
                "Revoked the {provider_kind} OAuth grant of integration connection {}",
                integration_connection.id
            ),
            GrantRevocationAttempt::AlreadyDead(detail) => info!(
                "The {provider_kind} OAuth grant of integration connection {} was already dead: {detail}",
                integration_connection.id
            ),
            GrantRevocationAttempt::Failed { tokens, error } => {
                warn!(
                    "Failed to revoke the {provider_kind} OAuth grant of integration connection {}, queuing it for retry: {error}",
                    integration_connection.id
                );
                let id = Uuid::new_v4();
                let (encrypted_access_token, encrypted_refresh_token) =
                    self.encrypt_grant_tokens(&tokens, id.as_bytes())?;
                self.repository
                    .create_oauth_grant_revocation(
                        executor,
                        NewOAuthGrantRevocation {
                            id,
                            provider_kind,
                            integration_connection_id: Some(integration_connection.id),
                            provider_user_id: integration_connection.provider_user_id.clone(),
                            provider_context: integration_connection.provider.context(),
                            encrypted_access_token,
                            encrypted_refresh_token,
                            access_token_expires_at: tokens.access_token_expires_at,
                            last_error: error,
                            next_attempt_at: Utc::now(),
                        },
                    )
                    .await?;
            }
        }

        Ok(())
    }

    /// Mark the Slack connections whose access Slack revoked (`tokens_revoked`
    /// for `provider_user_ids`, `app_uninstalled` for a whole workspace when
    /// `None`) as `Failing` and drop their now dead credential.
    ///
    /// Idempotent, as Slack may replay events and does not order them: a
    /// disconnected (`Created`) connection is left alone, and so is one whose
    /// credential was minted after `revoked_at` (the user reconnected since).
    /// Returns the number of connections that changed.
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            team_id = %team_id,
            provider_user_ids = ?provider_user_ids,
            revoked_at = revoked_at.to_rfc3339()
        ),
        err
    )]
    pub async fn revoke_slack_access(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        team_id: &str,
        provider_user_ids: Option<&[String]>,
        revoked_at: DateTime<Utc>,
    ) -> Result<usize, UniversalInboxError> {
        let integration_connections = self
            .repository
            .find_slack_integration_connections_per_team(executor, team_id, provider_user_ids)
            .await?;

        let mut revoked = 0;
        for integration_connection in integration_connections {
            // A paused connection has no credential left, and pausing it
            // revoked its token, which makes Slack send this very event.
            if matches!(
                integration_connection.status,
                IntegrationConnectionStatus::Created | IntegrationConnectionStatus::Paused
            ) {
                continue;
            }
            let credential = self
                .repository
                .get_oauth_credential(executor, integration_connection.id)
                .await?;
            if let Some(credential) = &credential {
                // Slack event times only have a one-second precision
                if credential.created_at.timestamp() > revoked_at.timestamp() {
                    debug!(
                        "Ignoring Slack revocation of integration connection {}: its credential is newer",
                        integration_connection.id
                    );
                    continue;
                }
                self.repository
                    .delete_oauth_credential(executor, integration_connection.id)
                    .await?;
            }

            let update = self
                .repository
                .update_integration_connection_status(
                    executor,
                    integration_connection.id,
                    IntegrationConnectionStatus::Failing,
                    Some(SLACK_ACCESS_REVOKED_ERROR_MESSAGE.to_string()),
                    None,
                    integration_connection.user_id,
                )
                .await?;
            if credential.is_some() || update.updated {
                info!(
                    "Slack access of integration connection {} was revoked, marked as Failing",
                    integration_connection.id
                );
                revoked += 1;
            }
        }

        Ok(revoked)
    }

    /// Revoke, at the providers, every OAuth grant of a user (account
    /// deletion). See [`Self::revoke_provider_grant`].
    #[tracing::instrument(level = "debug", skip_all, fields(user.id = user_id.to_string()), err)]
    pub async fn revoke_all_provider_grants(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let integration_connections = self
            .repository
            .fetch_all_integration_connections(executor, user_id, None, false)
            .await?;
        for integration_connection in &integration_connections {
            self.revoke_provider_grant(executor, integration_connection)
                .await?;
        }
        Ok(())
    }

    /// Retry the queued OAuth grant revocations that are due, up to
    /// `max_revocations`. Each one runs in its own transaction, holding its
    /// row lock (`FOR UPDATE SKIP LOCKED`) during the provider calls, so
    /// concurrent workers never retry the same grant.
    /// Returns `(completed_count, failed_count)`.
    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn retry_due_grant_revocations(
        &self,
        max_revocations: usize,
        retry_policy: &GrantRevocationRetryPolicy,
    ) -> Result<(usize, usize), UniversalInboxError> {
        let mut completed = 0usize;
        let mut failed = 0usize;

        for _ in 0..max_revocations {
            let mut transaction = self.begin().await?;
            let Some(revocation) = self
                .repository
                .claim_due_oauth_grant_revocation(&mut transaction, Utc::now())
                .await?
            else {
                break;
            };
            let revocation_id = revocation.id;

            if self
                .retry_grant_revocation(&mut transaction, revocation, retry_policy)
                .await?
            {
                completed += 1;
            } else {
                failed += 1;
            }

            transaction.commit().await.context(format!(
                "Failed to commit the retry of the OAuth grant revocation {revocation_id}"
            ))?;
        }

        Ok((completed, failed))
    }

    /// Retry one claimed revocation. Returns whether it left the Pending
    /// status for good (revoked, already dead or cancelled).
    async fn retry_grant_revocation(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        revocation: PendingOAuthGrantRevocation,
        retry_policy: &GrantRevocationRetryPolicy,
    ) -> Result<bool, UniversalInboxError> {
        let id = revocation.id;
        let provider_kind = revocation.provider_kind;

        // GitHub and Google revoke the whole grant, and Slack may hand the
        // same token back on reconnect: revoking now would kill the grant the
        // user just gave again.
        if self
            .repository
            .has_oauth_credential_for_provider_account(
                executor,
                provider_kind,
                revocation.integration_connection_id,
                revocation.provider_user_id.as_deref(),
            )
            .await?
        {
            info!(
                "The {provider_kind} account of OAuth grant revocation {id} was connected again, cancelling the revocation"
            );
            self.repository
                .complete_oauth_grant_revocation(
                    executor,
                    id,
                    true,
                    Some("The provider account was connected again".to_string()),
                )
                .await?;
            return Ok(true);
        }

        // Without a provider or a decryptable token, the stored tokens are
        // kept as they are.
        let unchanged = |error: String, abandon: bool| FailedGrantRevocation {
            encrypted_access_token: revocation.encrypted_access_token.clone(),
            encrypted_refresh_token: revocation.encrypted_refresh_token.clone(),
            access_token_expires_at: revocation.access_token_expires_at,
            error,
            abandon,
        };
        let failure = match self.get_oauth2_provider(&provider_kind) {
            // The provider may be configured again later: keep retrying.
            None => unchanged(
                format!("No OAuth2 provider configured for {provider_kind}"),
                false,
            ),
            Some(provider) => match self.decrypt_grant_tokens(
                &revocation.encrypted_access_token,
                revocation.encrypted_refresh_token.as_deref(),
                revocation.access_token_expires_at,
                id.as_bytes(),
            ) {
                // Undecryptable now means undecryptable forever.
                Err(err) => unchanged(format!("Failed to decrypt the tokens: {err:?}"), true),
                Ok(tokens) => match self.attempt_grant_revocation(provider, tokens).await {
                    GrantRevocationAttempt::Revoked => {
                        info!("Revoked the {provider_kind} OAuth grant of revocation {id}");
                        self.repository
                            .complete_oauth_grant_revocation(executor, id, false, None)
                            .await?;
                        return Ok(true);
                    }
                    GrantRevocationAttempt::AlreadyDead(detail) => {
                        info!(
                            "The {provider_kind} OAuth grant of revocation {id} was already dead: {detail}"
                        );
                        self.repository
                            .complete_oauth_grant_revocation(executor, id, false, Some(detail))
                            .await?;
                        return Ok(true);
                    }
                    GrantRevocationAttempt::Failed { tokens, error } => {
                        let (encrypted_access_token, encrypted_refresh_token) =
                            self.encrypt_grant_tokens(&tokens, id.as_bytes())?;
                        FailedGrantRevocation {
                            encrypted_access_token,
                            encrypted_refresh_token,
                            access_token_expires_at: tokens.access_token_expires_at,
                            error,
                            abandon: false,
                        }
                    }
                },
            },
        };

        let attempts = revocation.attempts + 1;
        let next_attempt_at = if failure.abandon {
            None
        } else {
            retry_policy.next_attempt_at(attempts)
        };
        let error = failure.error;
        if next_attempt_at.is_some() {
            warn!(
                "Failed attempt {attempts} to revoke the {provider_kind} OAuth grant of revocation {id}: {error}"
            );
        } else {
            error!(
                "Abandoning the revocation of the {provider_kind} OAuth grant of revocation {id} after {attempts} attempt(s): {error}"
            );
        }
        self.repository
            .record_oauth_grant_revocation_failure(
                executor,
                id,
                failure.encrypted_access_token,
                failure.encrypted_refresh_token,
                failure.access_token_expires_at,
                error,
                next_attempt_at,
            )
            .await?;
        Ok(false)
    }

    /// Revoke a grant at its provider. An expired access token is refreshed
    /// first when a refresh token exists, as some providers (Slack) only
    /// revoke with a live access token. A token the provider no longer
    /// accepts counts as done: there is nothing left to revoke.
    async fn attempt_grant_revocation(
        &self,
        provider: &dyn OAuth2Provider,
        mut tokens: GrantTokens,
    ) -> GrantRevocationAttempt {
        let mut refreshed = false;
        if tokens.refresh_token.is_some()
            && tokens
                .access_token_expires_at
                .is_some_and(|expires_at| expires_at <= Utc::now())
        {
            match self.refresh_grant_tokens(provider, &mut tokens).await {
                Ok(()) => refreshed = true,
                Err(attempt) => return attempt.with_tokens(tokens),
            }
        }

        loop {
            match self
                .oauth2_flow_service
                .revoke_token(
                    provider,
                    &tokens.access_token,
                    tokens.refresh_token.as_ref(),
                )
                .await
            {
                Ok(()) => return GrantRevocationAttempt::Revoked,
                Err(TokenRevocationError::TokenDead(detail)) => {
                    return GrantRevocationAttempt::AlreadyDead(detail);
                }
                Err(TokenRevocationError::AccessTokenExpired(_))
                    if !refreshed && tokens.refresh_token.is_some() =>
                {
                    match self.refresh_grant_tokens(provider, &mut tokens).await {
                        Ok(()) => refreshed = true,
                        Err(attempt) => return attempt.with_tokens(tokens),
                    }
                }
                Err(err) => {
                    return GrantRevocationAttempt::Failed {
                        tokens,
                        error: err.to_string(),
                    };
                }
            }
        }
    }

    /// Refresh `tokens` in place. A refresh token the provider rejects means
    /// the grant is already dead.
    async fn refresh_grant_tokens(
        &self,
        provider: &dyn OAuth2Provider,
        tokens: &mut GrantTokens,
    ) -> Result<(), RefreshFailure> {
        let Some(refresh_token) = &tokens.refresh_token else {
            return Err(RefreshFailure::Failed("No refresh token".to_string()));
        };
        match self
            .oauth2_flow_service
            .refresh_access_token(provider, refresh_token)
            .await
        {
            Ok(response) => {
                tokens.access_token = response.access_token.expose_secret().clone();
                if let Some(refresh_token) = &response.refresh_token {
                    tokens.refresh_token = Some(refresh_token.expose_secret().clone());
                }
                tokens.access_token_expires_at = response.expires_at();
                Ok(())
            }
            Err(UniversalInboxError::OAuth2InvalidGrant(detail)) => Err(RefreshFailure::Dead(
                format!("refresh token rejected: {detail}"),
            )),
            Err(err) => Err(RefreshFailure::Failed(format!(
                "Failed to refresh the access token: {err:?}"
            ))),
        }
    }

    fn decrypt_grant_tokens(
        &self,
        encrypted_access_token: &[u8],
        encrypted_refresh_token: Option<&[u8]>,
        access_token_expires_at: Option<DateTime<Utc>>,
        aad_context: &[u8],
    ) -> Result<GrantTokens, UniversalInboxError> {
        let token_encryption_key = self.token_encryption_key.expose_secret();
        let access_token = AccessToken(decrypt_token(
            encrypted_access_token,
            aad_context,
            token_encryption_key,
        )?);
        let refresh_token = encrypted_refresh_token
            .map(|encrypted| decrypt_token(encrypted, aad_context, token_encryption_key))
            .transpose()?
            .map(RefreshToken);
        Ok(GrantTokens {
            access_token,
            refresh_token,
            access_token_expires_at,
        })
    }

    fn encrypt_grant_tokens(
        &self,
        tokens: &GrantTokens,
        aad_context: &[u8],
    ) -> Result<(Vec<u8>, Option<Vec<u8>>), UniversalInboxError> {
        let token_encryption_key = self.token_encryption_key.expose_secret();
        let encrypted_access_token = encrypt_token(
            tokens.access_token.as_str(),
            aad_context,
            token_encryption_key,
        )?;
        let encrypted_refresh_token = tokens
            .refresh_token
            .as_ref()
            .map(|refresh_token| {
                encrypt_token(refresh_token.as_str(), aad_context, token_encryption_key)
            })
            .transpose()?;
        Ok((encrypted_access_token, encrypted_refresh_token))
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_connection_id = integration_connection_id.to_string(),
            user.id = for_user_id.to_string()
        ),
        err
    )]
    pub async fn disconnect_integration_connection(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError> {
        if let Some(integration_connection) = self
            .repository
            .get_integration_connection(executor, integration_connection_id)
            .await?
        {
            if integration_connection.user_id != for_user_id {
                return Err(UniversalInboxError::Forbidden(format!(
                    "Only the owner of the integration connection {integration_connection_id} can disconnect it"
                )));
            }

            // Tell the provider to drop the grant too, so it no longer lists
            // Universal Inbox as authorized and the refresh token dies on its
            // side. A provider failure never blocks the disconnect: the grant
            // is queued for retry instead.
            self.revoke_provider_grant(executor, &integration_connection)
                .await?;

            self.repository
                .delete_oauth_credential(executor, integration_connection_id)
                .await?;

            let disconnected_integration_connection = self
                .repository
                .update_integration_connection_status(
                    executor,
                    integration_connection_id,
                    IntegrationConnectionStatus::Created,
                    None,
                    None,
                    for_user_id,
                )
                .await?;

            // Disconnecting stops feeding the inbox just as muting does, so it
            // is no more destructive: the notifications leave the inbox and are
            // set aside, and reconnecting brings them back on the sync that
            // reconciles them.
            if let Some(disconnected_integration_connection) =
                &disconnected_integration_connection.result
            {
                self.reconcile_set_aside_notifications(
                    executor,
                    disconnected_integration_connection,
                )
                .await?;
            }

            return Ok(disconnected_integration_connection);
        }

        Ok(UpdateStatus {
            updated: false,
            result: None,
        })
    }

    /// Pause the `Validated` `provider_kind` connections of users inactive
    /// since `inactive_before`, see [`Self::pause_integration_connections`].
    /// Returns `(paused_count, failed_count)`.
    #[tracing::instrument(
        level = "info",
        skip(self),
        fields(
            provider_kind = provider_kind.to_string(),
            inactive_before = inactive_before.to_rfc3339()
        ),
        err
    )]
    pub async fn pause_integration_connections_of_inactive_users(
        &self,
        provider_kind: IntegrationProviderKind,
        inactive_before: DateTime<Utc>,
    ) -> Result<(usize, usize), UniversalInboxError> {
        let mut transaction = self.begin().await.context(
            "Failed to create new transaction while listing integration connections of inactive users",
        )?;
        let integration_connection_ids = self
            .repository
            .find_validated_integration_connections_of_inactive_users(
                &mut transaction,
                provider_kind,
                inactive_before,
            )
            .await?;
        transaction
            .commit()
            .await
            .context("Failed to commit while listing integration connections of inactive users")?;

        self.pause_integration_connections(
            provider_kind,
            integration_connection_ids,
            IntegrationConnectionPausedReason::Inactivity,
        )
        .await
    }

    /// Pause the `provider_kind` connections `Failing` since before
    /// `failing_before`, see [`Self::pause_integration_connections`]. A failing
    /// connection syncs nothing, but its grant may still be valid at the
    /// provider (Slack then keeps sending its events).
    /// Returns `(paused_count, failed_count)`.
    #[tracing::instrument(
        level = "info",
        skip(self),
        fields(
            provider_kind = provider_kind.to_string(),
            failing_before = failing_before.to_rfc3339()
        ),
        err
    )]
    pub async fn pause_long_failing_integration_connections(
        &self,
        provider_kind: IntegrationProviderKind,
        failing_before: DateTime<Utc>,
    ) -> Result<(usize, usize), UniversalInboxError> {
        let mut transaction = self.begin().await.context(
            "Failed to create new transaction while listing long failing integration connections",
        )?;
        let integration_connection_ids = self
            .repository
            .find_long_failing_integration_connections(
                &mut transaction,
                provider_kind,
                failing_before,
            )
            .await?;
        transaction
            .commit()
            .await
            .context("Failed to commit while listing long failing integration connections")?;

        self.pause_integration_connections(
            provider_kind,
            integration_connection_ids,
            IntegrationConnectionPausedReason::LongFailing,
        )
        .await
    }

    /// Revoke the grant of each connection at the provider (so Slack, for
    /// instance, stops sending events for it; a failed revocation is queued
    /// for retry), delete its credential and move it to `Paused`. Each connection is handled in its own transaction so
    /// that one failure does not hold back the others.
    async fn pause_integration_connections(
        &self,
        provider_kind: IntegrationProviderKind,
        integration_connection_ids: Vec<IntegrationConnectionId>,
        paused_reason: IntegrationConnectionPausedReason,
    ) -> Result<(usize, usize), UniversalInboxError> {
        info!(
            "Found {} {provider_kind} integration connection(s) to pause ({paused_reason})",
            integration_connection_ids.len()
        );

        let mut paused = 0usize;
        let mut failed = 0usize;
        for integration_connection_id in integration_connection_ids {
            let mut transaction = self.begin().await.context(format!(
                "Failed to create new transaction while pausing integration connection {integration_connection_id}"
            ))?;
            match self
                .pause_integration_connection(
                    &mut transaction,
                    integration_connection_id,
                    paused_reason,
                )
                .await
            {
                Ok(is_paused) => {
                    transaction.commit().await.context(format!(
                        "Failed to commit while pausing integration connection {integration_connection_id}"
                    ))?;
                    if is_paused {
                        paused += 1;
                    }
                }
                Err(err) => {
                    error!(
                        "Failed to pause {provider_kind} integration connection {integration_connection_id}: {err:?}"
                    );
                    failed += 1;
                }
            }
        }

        info!(
            "Paused {paused} {provider_kind} integration connection(s) ({paused_reason}), {failed} failed"
        );
        Ok((paused, failed))
    }

    /// Pause one connection, see [`Self::pause_integration_connections`].
    /// Returns whether it was paused: one the user disconnected or reconnected
    /// since it was listed is left alone.
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_connection_id = integration_connection_id.to_string(),
            paused_reason = paused_reason.to_string()
        ),
        err
    )]
    async fn pause_integration_connection(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        paused_reason: IntegrationConnectionPausedReason,
    ) -> Result<bool, UniversalInboxError> {
        let Some(integration_connection) = self
            .repository
            .get_integration_connection(executor, integration_connection_id)
            .await?
        else {
            return Ok(false);
        };
        let is_still_eligible = match paused_reason {
            IntegrationConnectionPausedReason::Inactivity => integration_connection.is_connected(),
            IntegrationConnectionPausedReason::LongFailing => integration_connection.is_failing(),
        };
        if !is_still_eligible {
            return Ok(false);
        }

        self.revoke_provider_grant(executor, &integration_connection)
            .await?;
        self.repository
            .delete_oauth_credential(executor, integration_connection_id)
            .await?;
        let paused_integration_connection = self
            .repository
            .pause_integration_connection(
                executor,
                integration_connection_id,
                Utc::now(),
                paused_reason,
            )
            .await?;

        // Pausing stops feeding the inbox just as disconnecting does.
        if let Some(paused_integration_connection) = &paused_integration_connection {
            self.reconcile_set_aside_notifications(executor, paused_integration_connection)
                .await?;
        }

        info!(
            "Paused the {} integration connection {integration_connection_id} of user {} ({paused_reason})",
            integration_connection.provider.kind(),
            integration_connection.user_id
        );
        Ok(true)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_provider_kind = integration_provider_kind.to_string(),
            min_sync_interval_in_minutes = min_sync_interval_in_minutes,
            sync_type = sync_type.to_string(),
            user.id = for_user_id.to_string()
        ),
        err
    )]
    pub async fn get_integration_connection_to_sync(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_provider_kind: IntegrationProviderKind,
        min_sync_interval_in_minutes: i64,
        sync_type: IntegrationConnectionSyncType,
        for_user_id: UserId,
    ) -> Result<Option<IntegrationConnection>, UniversalInboxError> {
        let synced_before = Utc::now()
            - TimeDelta::try_minutes(min_sync_interval_in_minutes).unwrap_or_else(|| {
                panic!(
                    "Invalid `min_sync_interval_in_minutes` value: {min_sync_interval_in_minutes}"
                )
            });

        let synced_before_filter = if min_sync_interval_in_minutes == 0 {
            None
        } else {
            match sync_type {
                IntegrationConnectionSyncType::Notifications => Some(
                    IntegrationConnectionSyncedBeforeFilter::Notifications(synced_before),
                ),
                IntegrationConnectionSyncType::Tasks => Some(
                    IntegrationConnectionSyncedBeforeFilter::Tasks(synced_before),
                ),
            }
        };
        let connection = self
            .repository
            .get_integration_connection_per_provider(
                executor,
                for_user_id,
                integration_provider_kind,
                synced_before_filter,
                Some(IntegrationConnectionStatus::Validated),
            )
            .await?;

        if let Some(ref conn) = connection {
            let in_backoff = match sync_type {
                IntegrationConnectionSyncType::Notifications => conn
                    .is_notifications_sync_in_backoff(
                        self.sync_backoff_base_delay_in_seconds,
                        self.sync_backoff_max_delay_in_seconds,
                    ),
                IntegrationConnectionSyncType::Tasks => conn.is_tasks_sync_in_backoff(
                    self.sync_backoff_base_delay_in_seconds,
                    self.sync_backoff_max_delay_in_seconds,
                ),
            };
            if in_backoff {
                debug!(
                    "{integration_provider_kind} {sync_type} sync for user {for_user_id} is in backoff, skipping"
                );
                return Ok(None);
            }
        }

        Ok(connection)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_provider_kind = integration_provider_kind.to_string(),
            user.id = for_user_id.to_string()
        ),
        err
    )]
    pub async fn get_validated_integration_connection_per_kind(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_provider_kind: IntegrationProviderKind,
        for_user_id: UserId,
    ) -> Result<Option<IntegrationConnection>, UniversalInboxError> {
        self.repository
            .get_integration_connection_per_provider(
                executor,
                for_user_id,
                integration_provider_kind,
                None,
                Some(IntegrationConnectionStatus::Validated),
            )
            .await
    }

    /// This function searches for a validated Slack integration connection with up-to-date
    /// registered OAuth scopes to access Slack API endpoints not related to a specific user.
    #[tracing::instrument(level = "debug", skip(self, executor), err)]
    pub async fn find_slack_access_token(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        context: IntegrationConnectionContext,
    ) -> Result<Option<(AccessToken, IntegrationConnection)>, UniversalInboxError> {
        let required_scopes = self
            .required_oauth_scopes
            .get(&IntegrationProviderKind::Slack)
            .map(|scopes| scopes.as_slice())
            .unwrap_or(&[]);

        let integration_connection = self
            .repository
            .get_integration_connection_per_context(executor, context, required_scopes)
            .await?;

        let Some(integration_connection) = integration_connection else {
            return Ok(None);
        };

        self.fetch_access_token_locally(executor, integration_connection)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_provider_kind = integration_provider_kind.to_string(),
            user.id = for_user_id.to_string()
        ),
        err
    )]
    pub async fn find_access_token(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_provider_kind: IntegrationProviderKind,
        for_user_id: UserId,
    ) -> Result<Option<(AccessToken, IntegrationConnection)>, UniversalInboxError> {
        let integration_connection = self
            .repository
            .get_integration_connection_per_provider(
                executor,
                for_user_id,
                integration_provider_kind,
                None,
                Some(IntegrationConnectionStatus::Validated),
            )
            .await?;

        let Some(integration_connection) = integration_connection else {
            return Ok(None);
        };

        self.fetch_access_token_locally(executor, integration_connection)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_connection.id = integration_connection_id.to_string(),
            user.id = for_user_id.to_string()
        ),
        err
    )]
    pub async fn find_access_token_for_connection(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        for_user_id: UserId,
    ) -> Result<Option<(AccessToken, IntegrationConnection)>, UniversalInboxError> {
        let integration_connection = self
            .repository
            .get_integration_connection(executor, integration_connection_id)
            .await?;

        let Some(integration_connection) = integration_connection else {
            return Ok(None);
        };

        if integration_connection.user_id != for_user_id {
            return Err(UniversalInboxError::Forbidden(format!(
                "Integration connection {integration_connection_id} does not belong to user {for_user_id}"
            )));
        }

        if integration_connection.status != IntegrationConnectionStatus::Validated {
            return Ok(None);
        }

        self.fetch_access_token_locally(executor, integration_connection)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_connection.id = integration_connection.id.to_string(),
        ),
        err
    )]
    async fn fetch_access_token_locally(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection: IntegrationConnection,
    ) -> Result<Option<(AccessToken, IntegrationConnection)>, UniversalInboxError> {
        let credential = self
            .repository
            .get_oauth_credential(executor, integration_connection.id)
            .await?;

        let Some(credential) = credential else {
            return Ok(None);
        };

        if let Some(expires_at) = credential.access_token_expires_at
            && expires_at < Utc::now()
        {
            if credential.encrypted_refresh_token.is_none() {
                self.repository
                    .update_integration_connection_status(
                        executor,
                        integration_connection.id,
                        IntegrationConnectionStatus::Failing,
                        Some(OAUTH_MISSING_REFRESH_TOKEN_ERROR_MESSAGE.to_string()),
                        None,
                        integration_connection.user_id,
                    )
                    .await?;

                return Err(UniversalInboxError::Recoverable(anyhow!(
                    "Access token expired for integration connection {} and no refresh token is stored. Marked connection as Failing; user must reconnect.",
                    integration_connection.id
                )));
            }

            return Err(UniversalInboxError::Recoverable(anyhow!(
                "Access token expired for integration connection {}. Token refresh should happen via the refresh-oauth-tokens command.",
                integration_connection.id
            )));
        }

        let token_encryption_key = self.token_encryption_key.expose_secret();
        let aad_context = integration_connection.id.0.as_bytes();
        let access_token = AccessToken(decrypt_token(
            &credential.encrypted_access_token,
            aad_context,
            token_encryption_key,
        )?);

        Ok(Some((access_token, integration_connection)))
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_connection_id = integration_connection_id.to_string()
        ),
        err
    )]
    pub async fn update_integration_connection_context(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        context: IntegrationConnectionContext,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError> {
        self.repository
            .update_integration_connection_context(
                executor,
                integration_connection_id,
                Some(context),
            )
            .await
    }

    pub async fn find_slack_integration_connections_without_context(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        for_user_id: Option<UserId>,
    ) -> Result<Vec<SlackIntegrationConnectionWithoutContext>, UniversalInboxError> {
        self.repository
            .find_slack_integration_connections_without_context(executor, for_user_id)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_provider_kind = integration_provider_kind.to_string()
        ),
        err
    )]
    pub async fn get_integration_connection_per_provider_user_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_provider_kind: IntegrationProviderKind,
        provider_user_id: String,
    ) -> Result<Option<IntegrationConnection>, UniversalInboxError> {
        self.repository
            .get_integration_connection_per_provider_user_id(
                executor,
                integration_provider_kind,
                provider_user_id,
            )
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_provider_kind = integration_provider_kind.to_string(),
        ),
        err
    )]
    pub async fn find_integration_connection_per_provider_user_ids(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_provider_kind: IntegrationProviderKind,
        provider_user_ids: Vec<String>,
    ) -> Result<Vec<IntegrationConnection>, UniversalInboxError> {
        self.repository
            .find_integration_connection_per_provider_user_ids(
                executor,
                integration_provider_kind,
                provider_user_ids,
            )
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_provider_kind = integration_provider_kind.map(|id| id.to_string()),
            user.id = for_user_id.map(|id| id.to_string())
        ),
        err
    )]
    pub async fn schedule_notifications_sync_status(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_provider_kind: Option<IntegrationProviderKind>,
        for_user_id: Option<UserId>,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError> {
        self.repository
            .update_integration_connection_sync_status(
                executor,
                for_user_id,
                integration_provider_kind,
                IntegrationConnectionSyncStatusUpdate::NotificationsSyncScheduled,
                self.sync_failure_window_in_hours,
            )
            .await
    }

    /// Atomically claims the start of a notifications sync for `integration_connection_id`:
    /// stamps `last_notifications_sync_started_at = now` and returns `true`, unless another
    /// caller already (re)started it more recently than `synced_before` (pass `None` to
    /// unconditionally reclaim, matching `force_sync`), in which case it returns `false` and
    /// this caller should back off rather than duplicate the sync.
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_connection_id = integration_connection_id.to_string(),
            user.id = for_user_id.to_string()
        ),
        err
    )]
    pub async fn claim_notification_sync_start(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        for_user_id: UserId,
        synced_before: Option<DateTime<Utc>>,
    ) -> Result<bool, UniversalInboxError> {
        let _ = for_user_id; // carried for the tracing field only; claim is by connection id
        self.repository
            .claim_notification_sync_start(
                executor,
                integration_connection_id,
                Utc::now(),
                synced_before,
            )
            .await
    }

    /// Tasks counterpart of [`Self::claim_notification_sync_start`].
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_connection_id = integration_connection_id.to_string(),
            user.id = for_user_id.to_string()
        ),
        err
    )]
    pub async fn claim_task_sync_start(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        for_user_id: UserId,
        synced_before: Option<DateTime<Utc>>,
    ) -> Result<bool, UniversalInboxError> {
        let _ = for_user_id; // carried for the tracing field only; claim is by connection id
        self.repository
            .claim_task_sync_start(
                executor,
                integration_connection_id,
                Utc::now(),
                synced_before,
            )
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_provider_kind = integration_provider_kind.to_string(),
            user.id = for_user_id.to_string()
        ),
        err
    )]
    pub async fn complete_notifications_sync_status(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_provider_kind: IntegrationProviderKind,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError> {
        let updated_integration_connection = self
            .repository
            .update_integration_connection_sync_status(
                executor,
                Some(for_user_id),
                Some(integration_provider_kind),
                IntegrationConnectionSyncStatusUpdate::NotificationsSyncCompleted,
                self.sync_failure_window_in_hours,
            )
            .await?;

        // The integration is feeding the inbox again and this sync has
        // reconciled it, so whatever was set aside comes back as it was.
        if let Some(integration_connection) = &updated_integration_connection.result {
            self.restore_set_aside_notifications(executor, integration_connection)
                .await?;
        }

        // Google Calendar notifications are derived during the Google Mail sync,
        // gated on the Google Calendar connection's own configuration, so this
        // is the only completion that reconciles them. The predicate evaluated
        // is that connection's own: a Google Calendar left disconnected keeps
        // its notifications set aside.
        if integration_provider_kind == IntegrationProviderKind::GoogleMail
            && let Some(google_calendar_integration_connection) = self
                .repository
                .get_integration_connection_per_provider(
                    executor,
                    for_user_id,
                    IntegrationProviderKind::GoogleCalendar,
                    None,
                    None,
                )
                .await?
        {
            self.restore_set_aside_notifications(executor, &google_calendar_integration_connection)
                .await?;
        }

        Ok(updated_integration_connection)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_provider_kind = integration_provider_kind.to_string(),
            user.id = for_user_id.to_string()
        ),
        err
    )]
    pub async fn error_notifications_sync_status(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_provider_kind: IntegrationProviderKind,
        failure_message: String,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError> {
        self.repository
            .update_integration_connection_sync_status(
                executor,
                Some(for_user_id),
                Some(integration_provider_kind),
                IntegrationConnectionSyncStatusUpdate::NotificationsSyncFailed(failure_message),
                self.sync_failure_window_in_hours,
            )
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_provider_kind = integration_provider_kind.map(|kind| kind.to_string()),
            user.id = for_user_id.map(|id| id.to_string())
        ),
        err
    )]
    pub async fn schedule_tasks_sync_status(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_provider_kind: Option<IntegrationProviderKind>,
        for_user_id: Option<UserId>,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError> {
        self.repository
            .update_integration_connection_sync_status(
                executor,
                for_user_id,
                integration_provider_kind,
                IntegrationConnectionSyncStatusUpdate::TasksSyncScheduled,
                self.sync_failure_window_in_hours,
            )
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_provider_kind = integration_provider_kind.to_string(),
            user.id = for_user_id.to_string()
        ),
        err
    )]
    pub async fn complete_tasks_sync_status(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_provider_kind: IntegrationProviderKind,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError> {
        let updated_integration_connection = self
            .repository
            .update_integration_connection_sync_status(
                executor,
                Some(for_user_id),
                Some(integration_provider_kind),
                IntegrationConnectionSyncStatusUpdate::TasksSyncCompleted,
                self.sync_failure_window_in_hours,
            )
            .await?;

        // Todoist and TickTick notifications are a byproduct of their task sync,
        // which never reaches `complete_notifications_sync_status` — so this is
        // the completion that reconciles them, and the only one that could bring
        // back what disconnecting them set aside. A provider whose notifications
        // have a sync of their own is left to it.
        if let Some(integration_connection) = &updated_integration_connection.result
            && integration_connection
                .provider
                .are_notifications_reconciled_by_tasks_sync()
        {
            self.restore_set_aside_notifications(executor, integration_connection)
                .await?;
        }

        Ok(updated_integration_connection)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_provider_kind = integration_provider_kind.to_string(),
            user.id = for_user_id.to_string()
        ),
    )]
    pub async fn error_tasks_sync_status(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_provider_kind: IntegrationProviderKind,
        failure_message: String,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError> {
        self.repository
            .update_integration_connection_sync_status(
                executor,
                Some(for_user_id),
                Some(integration_provider_kind),
                IntegrationConnectionSyncStatusUpdate::TasksSyncFailed(failure_message),
                self.sync_failure_window_in_hours,
            )
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            provider_kind = provider_kind.to_string()
        ),
        err
    )]
    pub async fn get_integration_connection_config_for_provider_user_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        provider_kind: IntegrationProviderKind,
        provider_user_id: String,
    ) -> Result<Option<IntegrationConnectionConfig>, UniversalInboxError> {
        // Using cache as the Slack event webhook will receive a lot of requests not related to Universal Inbox users
        cached_get_integration_connection_config_for_provider_user_id(
            self.repository.clone(),
            executor,
            provider_kind,
            provider_user_id,
        )
        .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_connection.id = %integration_connection_id,
            user.id = %user_id,
            status = ?status
        ),
        err
    )]
    pub async fn update_integration_connection_status(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        user_id: UserId,
        status: IntegrationConnectionStatus,
        registered_oauth_scopes: Vec<String>,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError> {
        // OAuth callbacks land here when an integration becomes Validated.
        // Enforce the Free-plan cap *again* — a user could race the limit by
        // initiating two OAuth flows in parallel, each of which started below
        // the cap; only the validated count matters for billing.
        if status == IntegrationConnectionStatus::Validated
            && let Some(billing) = &self.billing_service
            && let Some(connection) = self
                .repository
                .get_integration_connection(executor, integration_connection_id)
                .await?
        {
            billing
                .assert_can_add_integration(executor, user_id, connection.provider.kind())
                .await?;
        }

        self.repository
            .update_integration_connection_status(
                executor,
                integration_connection_id,
                status,
                None,
                Some(registered_oauth_scopes),
                user_id,
            )
            .await
    }

    /// Refresh all OAuth credentials expiring within `minutes_before_expiry` minutes.
    /// Optionally filter by `provider_kind`.
    /// Returns `(refreshed_count, failed_count)`.
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(
            minutes_before_expiry = minutes_before_expiry,
            integration.provider_kind = ?provider_kind
        ),
        err
    )]
    pub async fn refresh_expiring_tokens(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        minutes_before_expiry: i64,
        provider_kind: Option<IntegrationProviderKind>,
    ) -> Result<(usize, usize), UniversalInboxError> {
        let token_encryption_key = self.token_encryption_key.expose_secret();
        let flow_service = &self.oauth2_flow_service;

        let expiring_before = Utc::now()
            + TimeDelta::try_minutes(minutes_before_expiry).ok_or_else(|| {
                UniversalInboxError::Unexpected(anyhow!(
                    "Invalid minutes_before_expiry value: {minutes_before_expiry}"
                ))
            })?;

        let expiring_credentials = self
            .repository
            .list_expiring_credentials(executor, expiring_before, provider_kind)
            .await?;

        let total = expiring_credentials.len();
        info!("Found {total} expiring OAuth credential(s) to refresh (before {expiring_before})");

        let mut refreshed = 0usize;
        let mut failed = 0usize;

        for credential in expiring_credentials {
            let conn_id = credential.integration_connection_id;
            let pk = credential.provider_kind;

            let provider = match self.get_oauth2_provider(&pk) {
                Some(p) => p,
                None => {
                    warn!(
                        "No OAuth2Provider configured for {pk:?}, skipping credential for connection {conn_id}"
                    );
                    failed += 1;
                    continue;
                }
            };

            let aad_context = conn_id.0.as_bytes();
            let refresh_token = match decrypt_token(
                &credential.encrypted_refresh_token,
                aad_context,
                token_encryption_key,
            ) {
                Ok(t) => RefreshToken(t),
                Err(err) => {
                    error!("Failed to decrypt refresh token for connection {conn_id}: {err:?}");
                    failed += 1;
                    continue;
                }
            };

            let token_response = match flow_service
                .refresh_access_token(provider, &refresh_token)
                .await
            {
                Ok(resp) => resp,
                Err(UniversalInboxError::OAuth2InvalidGrant(detail)) => {
                    warn!(
                        "Refresh token for connection {conn_id} ({pk:?}) is no longer valid \
                         (invalid_grant): {detail}. Marking connection as Failing."
                    );
                    if let Err(update_err) = self
                        .repository
                        .update_integration_connection_status(
                            executor,
                            conn_id,
                            IntegrationConnectionStatus::Failing,
                            Some(OAUTH_INVALID_GRANT_ERROR_MESSAGE.to_string()),
                            None,
                            credential.user_id,
                        )
                        .await
                    {
                        error!(
                            "Failed to mark connection {conn_id} as Failing after invalid_grant: {update_err:?}"
                        );
                    }
                    failed += 1;
                    continue;
                }
                Err(err) => {
                    error!(
                        "Failed to refresh access token for connection {conn_id} ({pk:?}): {err:?}"
                    );
                    failed += 1;
                    continue;
                }
            };

            let encrypted_access_token = match encrypt_token(
                token_response.access_token.expose_secret().as_str(),
                aad_context,
                token_encryption_key,
            ) {
                Ok(t) => t,
                Err(err) => {
                    error!("Failed to encrypt new access token for connection {conn_id}: {err:?}");
                    failed += 1;
                    continue;
                }
            };

            let encrypted_refresh_token = match token_response
                .refresh_token
                .as_ref()
                .map(|rt| {
                    encrypt_token(
                        rt.expose_secret().as_str(),
                        aad_context,
                        token_encryption_key,
                    )
                })
                .transpose()
            {
                Ok(t) => t,
                Err(err) => {
                    error!("Failed to encrypt new refresh token for connection {conn_id}: {err:?}");
                    failed += 1;
                    continue;
                }
            };

            let expires_at = token_response.expires_at();
            // Same sanitization as the OAuth callback: some providers (Slack)
            // echo the whole token body in `extra`, cleartext tokens included.
            let raw_response = provider.sanitize_raw_response(
                &serde_json::to_value(token_response.as_safe_token_response()).unwrap_or_default(),
            );

            match self
                .repository
                .store_oauth_credential(
                    executor,
                    conn_id,
                    encrypted_access_token,
                    encrypted_refresh_token,
                    expires_at,
                    raw_response,
                )
                .await
            {
                Ok(_) => {
                    info!("Successfully refreshed OAuth token for connection {conn_id} ({pk:?})");
                    refreshed += 1;
                }
                Err(err) => {
                    error!("Failed to store refreshed token for connection {conn_id}: {err:?}");
                    failed += 1;
                }
            }
        }

        info!("Token refresh complete: {refreshed} refreshed, {failed} failed out of {total}");
        Ok((refreshed, failed))
    }

    pub async fn start_oauth_authorization(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        user_id: UserId,
        cache: &Cache,
    ) -> Result<Url, UniversalInboxError> {
        // Validate the integration connection exists, belongs to user, has status Created
        let integration_connection = self
            .get_integration_connection(executor, integration_connection_id)
            .await?
            .ok_or_else(|| {
                UniversalInboxError::ItemNotFound(format!(
                    "Integration connection {integration_connection_id} not found"
                ))
            })?;

        if integration_connection.user_id != user_id {
            return Err(UniversalInboxError::Forbidden(format!(
                "Integration connection {integration_connection_id} does not belong to user {user_id}"
            )));
        }

        if integration_connection.status != IntegrationConnectionStatus::Created {
            return Err(UniversalInboxError::UnsupportedAction(format!(
                "Integration connection {integration_connection_id} is not in Created status"
            )));
        }

        // Free-plan cap, enforced *before* the provider redirect. Reconnecting
        // an existing `Created` row skips `create_integration_connection`, so
        // without this the only check left was the OAuth callback: the user
        // would approve the app at the provider and be refused on the way back,
        // having spent a full round trip to learn they are over the cap.
        let provider_kind = integration_connection.provider.kind();

        if let Some(billing) = &self.billing_service {
            billing
                .assert_can_add_integration(executor, user_id, provider_kind)
                .await?;
        }

        // Look up the OAuth2Provider for this provider kind
        let provider = self.get_oauth2_provider(&provider_kind).ok_or_else(|| {
            UniversalInboxError::UnsupportedAction(format!(
                "No OAuth2 provider configured for {provider_kind:?}"
            ))
        })?;

        let redirect_uri = self.oauth2_flow_service.redirect_uri();

        // Build an OAuth2 client from the provider configuration
        let client = oauth2::basic::BasicClient::new(oauth2::ClientId::new(
            provider.client_id().to_string(),
        ))
        .set_client_secret(oauth2::ClientSecret::new(
            provider.client_secret().expose_secret().0.clone(),
        ))
        .set_auth_uri(
            oauth2::AuthUrl::new(provider.authorize_url().to_string())
                .expect("OAuth2Provider authorize_url is already a valid URL"),
        )
        .set_token_uri(
            oauth2::TokenUrl::new(provider.token_url().to_string())
                .expect("OAuth2Provider token_url is already a valid URL"),
        )
        .set_redirect_uri(
            oauth2::RedirectUrl::new(redirect_uri.to_string())
                .expect("oauth_redirect_uri is already a valid URL"),
        );

        let mut auth_request = client.authorize_url(CsrfToken::new_random);

        // Add scopes as an extra param (each provider controls delimiter & param name)
        let scopes = provider.required_scopes();
        if !scopes.is_empty() {
            auth_request = auth_request.add_extra_param(
                provider.scope_param_name(),
                scopes.join(provider.scope_delimiter()),
            );
        }

        // Provider-specific extra authorize params (e.g. Google's access_type=offline)
        for (key, value) in provider.extra_authorize_params() {
            auth_request = auth_request.add_extra_param(key, value);
        }

        // Add PKCE if supported
        let pkce_verifier = if provider.supports_pkce() {
            let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
            auth_request = auth_request.set_pkce_challenge(challenge);
            Some(SecretBox::new(Box::new(PkceVerifier(
                verifier.into_secret().to_string(),
            ))))
        } else {
            None
        };

        let (authorization_url, csrf_state) = auth_request.url();

        let state = csrf_state.into_secret().to_string();
        let state_data = OAuthStateData {
            integration_connection_id,
            pkce_verifier,
            provider_kind,
            user_id: Some(user_id),
        };
        let state_json =
            serde_json::to_string(&state_data).context("Failed to serialize OAuth state data")?;

        let redis_key = format!("{OAUTH_STATE_PREFIX}{state}");
        let mut conn = cache.connection_manager.clone();
        conn.set_ex::<_, _, ()>(&redis_key, &state_json, OAUTH_STATE_TTL_SECONDS)
            .await
            .context("Failed to store OAuth state in Redis")?;

        Ok(authorization_url)
    }

    /// Finish an integration OAuth flow for `user_id`, the user whose session
    /// delivered the callback. The state must have been issued to that user
    /// and name a connection they own; otherwise the authorization code is
    /// not exchanged and nothing is stored.
    pub async fn complete_oauth_callback(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        authorization_code: &SecretBox<AuthorizationCode>,
        state: &str,
        cache: &Cache,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // Look up and delete state from Redis (single-use)
        let redis_key = format!("{OAUTH_STATE_PREFIX}{state}");
        let mut conn = cache.connection_manager.clone();
        let state_json: Option<String> = conn
            .get_del(&redis_key)
            .await
            .context("Failed to retrieve OAuth state from Redis")?;

        let state_json = state_json.ok_or_else(|| {
            UniversalInboxError::Unauthorized(anyhow::anyhow!("Invalid or expired OAuth state"))
        })?;

        let state_data: OAuthStateData =
            serde_json::from_str(&state_json).context("Failed to deserialize OAuth state data")?;

        if state_data.user_id != Some(user_id) {
            return Err(UniversalInboxError::Unauthorized(anyhow::anyhow!(
                "OAuth state for integration connection {} was not issued to user {user_id}",
                state_data.integration_connection_id
            )));
        }

        // Get the integration connection and verify it belongs to the caller
        // and is still in Created status, before exchanging the code. The
        // status check prevents a late callback from a duplicate authorize
        // flow from overwriting credentials stored by an earlier successful
        // callback.
        let integration_connection = self
            .get_integration_connection(executor, state_data.integration_connection_id)
            .await?
            .ok_or_else(|| {
                UniversalInboxError::Unexpected(anyhow::anyhow!(
                    "Integration connection {} not found",
                    state_data.integration_connection_id
                ))
            })?;

        if integration_connection.user_id != user_id {
            return Err(UniversalInboxError::Unauthorized(anyhow::anyhow!(
                "Integration connection {} does not belong to user {user_id}",
                state_data.integration_connection_id
            )));
        }

        if integration_connection.status != IntegrationConnectionStatus::Created {
            return Err(UniversalInboxError::UnsupportedAction(format!(
                "Integration connection {} is no longer in Created status (current: {:?}), ignoring stale OAuth callback",
                state_data.integration_connection_id, integration_connection.status
            )));
        }

        let provider = self
            .get_oauth2_provider(&state_data.provider_kind)
            .ok_or_else(|| {
                UniversalInboxError::Unexpected(anyhow::anyhow!(
                    "No OAuth2 provider configured for {:?}",
                    state_data.provider_kind
                ))
            })?;

        let token_encryption_key = self.token_encryption_key.expose_secret();

        let token_response = self
            .oauth2_flow_service
            .exchange_code_for_token(
                provider,
                authorization_code,
                state_data.pkce_verifier.as_ref(),
            )
            .await?;

        let raw_response = serde_json::to_value(token_response.as_safe_token_response())
            .context("Failed to serialize token response to Value")?;

        let provider_user_id = match provider.extract_provider_user_id(&raw_response) {
            Some(provider_user_id) => Some(provider_user_id),
            None => {
                self.oauth2_flow_service
                    .fetch_provider_user_id(provider, token_response.access_token.expose_secret())
                    .await?
            }
        };

        // Reconnecting with another provider account than the pinned one is
        // allowed: the connection is re-pinned to the new account below, and
        // provider contexts cached from the previous account are refreshed on
        // the next sync. Identities are not logged.
        if let (Some(pinned_provider_user_id), Some(provider_user_id)) = (
            integration_connection.provider_user_id.as_ref(),
            provider_user_id.as_ref(),
        ) && pinned_provider_user_id != provider_user_id
        {
            warn!(
                "Pinned {} identity of integration connection {} changed on reconnect, re-pinning it",
                state_data.provider_kind, state_data.integration_connection_id
            );
        }

        // Encrypt tokens (bind ciphertext to this specific connection via AAD)
        let aad_context = state_data.integration_connection_id.0.as_bytes();
        let encrypted_access_token = encrypt_token(
            token_response.access_token.expose_secret().as_str(),
            aad_context,
            token_encryption_key,
        )?;
        let encrypted_refresh_token = token_response
            .refresh_token
            .as_ref()
            .map(|rt| {
                encrypt_token(
                    rt.expose_secret().as_str(),
                    aad_context,
                    token_encryption_key,
                )
            })
            .transpose()?;

        let expires_at = token_response.expires_at();

        let registered_scopes = provider.extract_registered_scopes(&raw_response)?;

        let stored_raw_response = provider.sanitize_raw_response(&raw_response);
        self.repository
            .store_oauth_credential(
                executor,
                state_data.integration_connection_id,
                encrypted_access_token,
                encrypted_refresh_token,
                expires_at,
                stored_raw_response,
            )
            .await?;

        if let Some(provider_user_id) = provider_user_id {
            self.repository
                .update_integration_connection_provider_user_id(
                    executor,
                    state_data.integration_connection_id,
                    Some(provider_user_id),
                )
                .await?;
        }

        if let Some(provider_context) = provider.extract_provider_context(&raw_response) {
            self.repository
                .update_integration_connection_context(
                    executor,
                    state_data.integration_connection_id,
                    Some(provider_context),
                )
                .await?;
        }

        self.update_integration_connection_status(
            executor,
            state_data.integration_connection_id,
            integration_connection.user_id,
            IntegrationConnectionStatus::Validated,
            registered_scopes,
        )
        .await?;

        Ok(())
    }
}

#[concurrent_cached(
    key = "String",
    convert = r#"{ format!("{}{}", provider_kind, provider_user_id) }"#,
    ty = "cached::AsyncRedisCache<String, Option<IntegrationConnectionConfig>>",
    map_error = r##"|e| UniversalInboxError::Unexpected(anyhow!("Failed to cache Slack `is_known_provider_user_id`: {:?}", e))"##,
    create = r##" { build_redis_cache("slack:is_known_provider_user_id", Duration::from_secs(6 * 60 * 60), false).await }"##
)]
async fn cached_get_integration_connection_config_for_provider_user_id(
    repository: Arc<Repository>,
    executor: &mut Transaction<'_, Postgres>,
    provider_kind: IntegrationProviderKind,
    provider_user_id: String,
) -> Result<Option<IntegrationConnectionConfig>, UniversalInboxError> {
    let integration_connection = repository
        .get_integration_connection_per_provider_user_id(executor, provider_kind, provider_user_id)
        .await?;

    Ok(integration_connection.map(|connection| {
        // A plan-paused connection answers with every sync off, whatever its
        // stored config says, so config state is not the only thing enforcing
        // a pause.
        if connection.auto_paused_by_plan_at.is_some() {
            let mut provider = connection.provider;
            provider.disable_all_syncs();
            provider.config()
        } else {
            connection.provider.config()
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grant_revocation_retry_policy_backs_off_exponentially() {
        let policy = GrantRevocationRetryPolicy {
            base_delay_in_seconds: 60,
            max_delay_in_seconds: 300,
            max_attempts: 5,
        };
        let delay = |attempts| {
            policy
                .next_attempt_at(attempts)
                .map(|next_attempt_at| (next_attempt_at - Utc::now()).num_seconds())
        };

        assert!(matches!(delay(1), Some(59..=60)));
        assert!(matches!(delay(2), Some(119..=120)));
        assert!(matches!(delay(3), Some(239..=240)));
        assert!(
            matches!(delay(4), Some(299..=300)),
            "capped at the max delay"
        );
        assert_eq!(delay(5), None, "abandoned after max_attempts");
    }
}
