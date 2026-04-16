use core::fmt;
use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Context, anyhow};
use apalis::prelude::*;
use apalis_redis::RedisStorage;
use cached::proc_macro::io_cached;
use chrono::{TimeDelta, Utc};
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

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionId, IntegrationConnectionStatus,
        NangoProviderKey,
        config::IntegrationConnectionConfig,
        provider::{IntegrationConnectionContext, IntegrationProviderKind},
    },
    notification::NotificationSyncSourceKind,
    task::TaskSyncSourceKind,
    user::UserId,
};

use crate::{
    integrations::oauth2::{
        AccessToken, AuthorizationCode, NangoService, PkceVerifier, RefreshToken,
        provider::{OAuth2FlowService, OAuth2Provider},
    },
    jobs::{
        UniversalInboxJob,
        sync::{SyncNotificationsJob, SyncTasksJob},
    },
    repository::{
        Repository,
        integration_connection::{
            IntegrationConnectionRepository, IntegrationConnectionSyncStatusUpdate,
            IntegrationConnectionSyncedBeforeFilter,
        },
        notification::NotificationRepository,
        oauth_credential::OAuthCredentialRepository,
    },
    universal_inbox::{UniversalInboxError, UpdateStatus, user::service::UserService},
    utils::{
        cache::{Cache, build_redis_cache},
        crypto::{TokenEncryptionKey, decrypt_token, encrypt_token},
    },
};

const OAUTH_STATE_PREFIX: &str = "universal-inbox::oauth-state::";
const OAUTH_STATE_TTL_SECONDS: u64 = 600;

#[derive(Debug, Serialize, Deserialize)]
struct OAuthStateData {
    integration_connection_id: IntegrationConnectionId,
    pkce_verifier: Option<SecretBox<PkceVerifier>>,
    provider_kind: IntegrationProviderKind,
}

pub struct IntegrationConnectionService {
    repository: Arc<Repository>,
    nango_service: NangoService,
    nango_provider_keys: HashMap<IntegrationProviderKind, NangoProviderKey>,
    required_oauth_scopes: HashMap<IntegrationProviderKind, Vec<String>>,
    oauth2_providers: HashMap<IntegrationProviderKind, Arc<dyn OAuth2Provider>>,
    oauth2_flow_service: Option<OAuth2FlowService>,
    token_encryption_key: Option<SecretBox<TokenEncryptionKey>>,
    user_service: Arc<UserService>,
    min_sync_notifications_interval_in_minutes: i64,
    min_sync_tasks_interval_in_minutes: i64,
    sync_backoff_base_delay_in_seconds: u64,
    sync_backoff_max_delay_in_seconds: u64,
    sync_failure_window_in_hours: i64,
}

pub const UNKNOWN_NANGO_CONNECTION_ERROR_MESSAGE: &str = "🔌 The OAuth connection is failing due to a technical issue on our end. Please try to reconnect the integration. If the issue keeps happening, please contact our support.";

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
        nango_service: NangoService,
        nango_provider_keys: HashMap<IntegrationProviderKind, NangoProviderKey>,
        required_oauth_scopes: HashMap<IntegrationProviderKind, Vec<String>>,
        oauth2_providers: HashMap<IntegrationProviderKind, Arc<dyn OAuth2Provider>>,
        oauth2_flow_service: Option<OAuth2FlowService>,
        token_encryption_key: Option<SecretBox<TokenEncryptionKey>>,
        user_service: Arc<UserService>,
        min_sync_notifications_interval_in_minutes: i64,
        min_sync_tasks_interval_in_minutes: i64,
        sync_backoff_base_delay_in_seconds: u64,
        sync_backoff_max_delay_in_seconds: u64,
        sync_failure_window_in_hours: i64,
    ) -> IntegrationConnectionService {
        IntegrationConnectionService {
            repository,
            nango_service,
            nango_provider_keys,
            required_oauth_scopes,
            oauth2_providers,
            oauth2_flow_service,
            token_encryption_key,
            user_service,
            min_sync_notifications_interval_in_minutes,
            min_sync_tasks_interval_in_minutes,
            sync_backoff_base_delay_in_seconds,
            sync_backoff_max_delay_in_seconds,
            sync_failure_window_in_hours,
        }
    }

    pub async fn begin(&self) -> Result<Transaction<'_, Postgres>, UniversalInboxError> {
        self.repository.begin().await
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

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = for_user_id.to_string()),
        err
    )]
    pub async fn trigger_sync_for_integration_connections(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        for_user_id: UserId,
        mut job_storage: RedisStorage<UniversalInboxJob>,
    ) -> Result<(), UniversalInboxError> {
        let mut integration_connections = self
            .fetch_all_integration_connections(
                executor,
                for_user_id,
                Some(IntegrationConnectionStatus::Validated),
                false,
            )
            .await?;
        for integration_connection in integration_connections.iter_mut() {
            if integration_connection.is_connected() {
                if let Ok(notification_sync_source_kind) =
                    integration_connection.provider.kind().try_into()
                {
                    let synced_before = Utc::now()
                            - TimeDelta::try_minutes(self.min_sync_notifications_interval_in_minutes)
                                .unwrap_or_else(|| {
                                    panic!(
                                        "Invalid `min_sync_notifications_interval_in_minutes` value: {}",
                                        self.min_sync_notifications_interval_in_minutes
                                    )
                                });

                    if integration_connection
                        .last_notifications_sync_scheduled_at
                        .map(|scheduled_at| scheduled_at <= synced_before)
                        .unwrap_or(true)
                    {
                        self.trigger_sync_notifications(
                            executor,
                            Some(notification_sync_source_kind),
                            Some(for_user_id),
                            &mut job_storage,
                        )
                        .await?;
                    }
                }
                if let Ok(task_sync_source_kind) = integration_connection.provider.kind().try_into()
                {
                    let synced_before = Utc::now()
                        - TimeDelta::try_minutes(self.min_sync_tasks_interval_in_minutes)
                            .unwrap_or_else(|| {
                                panic!(
                                    "Invalid `min_sync_tasks_interval_in_minutes` value: {}",
                                    self.min_sync_tasks_interval_in_minutes
                                )
                            });

                    if integration_connection
                        .last_tasks_sync_scheduled_at
                        .map(|scheduled_at| scheduled_at <= synced_before)
                        .unwrap_or(true)
                    {
                        self.trigger_sync_tasks(
                            executor,
                            Some(task_sync_source_kind),
                            Some(for_user_id),
                            &mut job_storage,
                        )
                        .await?;
                    }
                }
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

        Retry::spawn(
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

        Retry::spawn(
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
        let updated_integration_connection_config = self
            .repository
            .update_integration_connection_config(
                executor,
                integration_connection_id,
                integration_connection_config.clone(),
                for_user_id,
            )
            .await?;

        if updated_integration_connection_config
            == (UpdateStatus {
                updated: false,
                result: None,
            })
        {
            if self
                .repository
                .does_integration_connection_exist(executor, integration_connection_id)
                .await?
            {
                return Err(UniversalInboxError::Forbidden(format!(
                    "Only the owner of the integration connection {integration_connection_id} can patch it"
                )));
            }
        } else if let Some(kind) = integration_connection_config.notification_source_kind() {
            self.repository
                .delete_notifications(executor, kind, for_user_id)
                .await?;
            self.repository
                .update_integration_connection_context(executor, integration_connection_id, None)
                .await?;
        }

        Ok(updated_integration_connection_config)
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
    pub async fn verify_integration_connection(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError> {
        let Some(integration_connection) = self
            .repository
            .get_integration_connection(executor, integration_connection_id)
            .await?
        else {
            return Ok(UpdateStatus {
                updated: false,
                result: None,
            });
        };

        if integration_connection.user_id != for_user_id {
            return Err(UniversalInboxError::Forbidden(format!(
                "Only the owner of the integration connection {integration_connection_id} can verify it"
            )));
        }

        let provider_kind = integration_connection.provider.kind();
        let provider_config_key = self
            .nango_provider_keys
            .get(&provider_kind)
            .context(format!(
                "No Nango provider config key found for {provider_kind}"
            ))?;

        let nango_connection = self
            .nango_service
            .get_connection(integration_connection.connection_id, provider_config_key)
            .await?;
        let Some(nango_connection) = nango_connection else {
            return self
                .repository
                .update_integration_connection_status(
                    executor,
                    integration_connection_id,
                    IntegrationConnectionStatus::Failing,
                    Some(UNKNOWN_NANGO_CONNECTION_ERROR_MESSAGE.to_string()),
                    None,
                    for_user_id,
                )
                .await;
        };

        if let Some(provider_user_id) = nango_connection.get_provider_user_id() {
            self.repository
                .update_integration_connection_provider_user_id(
                    executor,
                    integration_connection_id,
                    Some(provider_user_id),
                )
                .await?;
        }

        if let Some(provider_context) = nango_connection.get_provider_context() {
            self.repository
                .update_integration_connection_context(
                    executor,
                    integration_connection_id,
                    Some(provider_context),
                )
                .await?;
        }

        self.repository
            .update_integration_connection_status(
                executor,
                integration_connection_id,
                IntegrationConnectionStatus::Validated,
                None,
                Some(nango_connection.get_registered_oauth_scopes()?),
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
                    "Only the owner of the integration connection {integration_connection_id} can verify it"
                )));
            }

            let provider_kind = integration_connection.provider.kind();

            if self.oauth2_providers.contains_key(&provider_kind) {
                self.repository
                    .delete_oauth_credential(executor, integration_connection_id)
                    .await?;
            } else {
                let provider_config_key =
                    self.nango_provider_keys
                        .get(&provider_kind)
                        .context(format!(
                            "No Nango provider config key found for {provider_kind}"
                        ))?;

                self.nango_service
                    .delete_connection(integration_connection.connection_id, provider_config_key)
                    .await?;
            }

            return self
                .repository
                .update_integration_connection_status(
                    executor,
                    integration_connection_id,
                    IntegrationConnectionStatus::Created,
                    None,
                    None,
                    for_user_id,
                )
                .await;
        }

        Ok(UpdateStatus {
            updated: false,
            result: None,
        })
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_provider_kind = integration_provider_kind.to_string(),
            min_sync_interval_in_minutes,
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

        self.fetch_access_token_from_nango(executor, integration_connection, None)
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

        self.fetch_access_token_for_integration_connection(
            executor,
            integration_connection,
            for_user_id,
        )
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

        self.fetch_access_token_for_integration_connection(
            executor,
            integration_connection,
            for_user_id,
        )
        .await
    }

    async fn fetch_access_token_for_integration_connection(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection: IntegrationConnection,
        for_user_id: UserId,
    ) -> Result<Option<(AccessToken, IntegrationConnection)>, UniversalInboxError> {
        let integration_provider_kind = integration_connection.provider.kind();
        if self
            .oauth2_providers
            .contains_key(&integration_provider_kind)
        {
            // Try local credential first; if none exists yet (unmigrated connection),
            // fall back to Nango so both migrated and unmigrated connections work
            // during the transition period.
            let result = self
                .fetch_access_token_locally(
                    executor,
                    integration_connection.clone(),
                    Some(for_user_id),
                )
                .await?;
            if result.is_some() {
                return Ok(result);
            }
            self.fetch_access_token_from_nango(executor, integration_connection, Some(for_user_id))
                .await
        } else {
            self.fetch_access_token_from_nango(executor, integration_connection, Some(for_user_id))
                .await
        }
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
        _for_user_id: Option<UserId>,
    ) -> Result<Option<(AccessToken, IntegrationConnection)>, UniversalInboxError> {
        let credential = self
            .repository
            .get_oauth_credential(executor, integration_connection.id)
            .await?;

        let Some(credential) = credential else {
            return Ok(None);
        };

        // Check if access token is expired
        if let Some(expires_at) = credential.access_token_expires_at
            && expires_at < Utc::now()
        {
            // Token expired - the eager refresh command should handle this
            return Err(UniversalInboxError::Recoverable(anyhow!(
                "Access token expired for integration connection {}. Token refresh should happen via the refresh-oauth-tokens command.",
                integration_connection.id
            )));
        }

        // Decrypt the access token
        let token_encryption_key = self
            .token_encryption_key
            .as_ref()
            .map(|k| k.expose_secret())
            .ok_or_else(|| {
                UniversalInboxError::Unexpected(anyhow!(
                    "Token encryption key not configured but required for local OAuth credentials"
                ))
            })?;

        let aad_context = integration_connection.id.0.as_bytes();
        let access_token = AccessToken(decrypt_token(
            &credential.encrypted_access_token,
            aad_context,
            token_encryption_key,
        )?);

        Ok(Some((access_token, integration_connection)))
    }

    async fn fetch_access_token_from_nango(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection: IntegrationConnection,
        for_user_id: Option<UserId>,
    ) -> Result<Option<(AccessToken, IntegrationConnection)>, UniversalInboxError> {
        let provider_kind = integration_connection.provider.kind();
        let provider_config_key = self
            .nango_provider_keys
            .get(&provider_kind)
            .context(format!(
                "No Nango provider config key found for {provider_kind}"
            ))?;

        let Some(nango_connection) = self
            .nango_service
            .get_connection(integration_connection.connection_id, provider_config_key)
            .await?
        else {
            // Only mark the connection as failing if we have a user_id to notify
            if let Some(for_user_id) = for_user_id {
                self.repository
                    .update_integration_connection_status(
                        executor,
                        integration_connection.id,
                        IntegrationConnectionStatus::Failing,
                        Some(UNKNOWN_NANGO_CONNECTION_ERROR_MESSAGE.to_string()),
                        None,
                        for_user_id,
                    )
                    .await?;
            }

            return Err(UniversalInboxError::Recoverable(anyhow!(
                "Unknown Nango connection: {}",
                integration_connection.connection_id
            )));
        };

        // This is only useful to update incomplete connection context as it was added
        // during the validation afterward
        if integration_connection.provider.context_is_empty()
            && let Some(provider_context) = nango_connection.get_provider_context()
        {
            self.repository
                .update_integration_connection_context(
                    executor,
                    integration_connection.id,
                    Some(provider_context),
                )
                .await?;
        }

        if provider_kind == IntegrationProviderKind::Slack
            && let Some(access_token) =
                nango_connection.credentials.raw["authed_user"]["access_token"].as_str()
        {
            return Ok(Some((
                AccessToken(access_token.to_string()),
                integration_connection,
            )));
        }

        Ok(Some((
            nango_connection.credentials.access_token,
            integration_connection,
        )))
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

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_provider_kind = integration_provider_kind.to_string(),
            provider_user_id,
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
            provider_user_ids,
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

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_provider_kind = integration_provider_kind.to_string(),
            user.id = for_user_id.to_string()
        ),
        err
    )]
    pub async fn start_notifications_sync_status(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_provider_kind: IntegrationProviderKind,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError> {
        self.repository
            .update_integration_connection_sync_status(
                executor,
                Some(for_user_id),
                Some(integration_provider_kind),
                IntegrationConnectionSyncStatusUpdate::NotificationsSyncStarted,
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
    pub async fn complete_notifications_sync_status(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_provider_kind: IntegrationProviderKind,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError> {
        self.repository
            .update_integration_connection_sync_status(
                executor,
                Some(for_user_id),
                Some(integration_provider_kind),
                IntegrationConnectionSyncStatusUpdate::NotificationsSyncCompleted,
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
    pub async fn start_tasks_sync_status(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_provider_kind: IntegrationProviderKind,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError> {
        self.repository
            .update_integration_connection_sync_status(
                executor,
                Some(for_user_id),
                Some(integration_provider_kind),
                IntegrationConnectionSyncStatusUpdate::TasksSyncStarted,
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
        self.repository
            .update_integration_connection_sync_status(
                executor,
                Some(for_user_id),
                Some(integration_provider_kind),
                IntegrationConnectionSyncStatusUpdate::TasksSyncCompleted,
                self.sync_failure_window_in_hours,
            )
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            integration_provider_kind = integration_provider_kind.to_string(),
            failure_message,
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
        fields(provider_kind = provider_kind.map(|kind| kind.to_string())),
        err
    )]
    pub async fn sync_oauth_scopes_for_all_users(
        &self,
        provider_kind: Option<IntegrationProviderKind>,
    ) -> Result<(), UniversalInboxError> {
        let service = self.user_service.clone();
        let mut transaction = service
            .begin()
            .await
            .context("Failed to create new transaction while syncing OAuth scopes for all users")?;
        let users = service.fetch_all_users(&mut transaction).await?;

        for user in users {
            let _ = self
                .sync_oauth_scopes_for_user(provider_kind, user.id)
                .await;
        }

        Ok(())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            provider_kind = provider_kind.map(|kind| kind.to_string()),
            user.id = user_id.to_string()
        ),
        err
    )]
    pub async fn sync_oauth_scopes_for_user(
        &self,
        provider_kind: Option<IntegrationProviderKind>,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        info!("Syncing OAuth scopes for user {user_id}");

        let mut transaction = self.begin().await.context(format!(
            "Failed to create new transaction while syncing {provider_kind:?} OAuth scopes"
        ))?;

        match self
            .sync_oauth_scopes(&mut transaction, provider_kind, user_id)
            .await
        {
            Ok(_) => {
                transaction.commit().await.context(format!(
                    "Failed to commit while syncing {provider_kind:?} OAuth scopes"
                ))?;
                info!("Successfully synced OAuth scopes for user {user_id}");
                Ok(())
            }
            Err(error @ UniversalInboxError::Recoverable(_)) => {
                transaction.commit().await.context(format!(
                    "Failed to commit while syncing {provider_kind:?} OAuth scopes"
                ))?;
                error!("Failed to sync OAuth scopes for user {user_id}: {error:?}");
                Err(error)
            }
            Err(error) => {
                transaction.rollback().await.context(format!(
                    "Failed to rollback while syncing {provider_kind:?} OAuth scopes"
                ))?;
                error!("Failed to sync OAuth scopes for user {user_id}: {error:?}");
                Err(error)
            }
        }
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            provider_kind = provider_kind.map(|kind| kind.to_string()),
            user.id = user_id.to_string()
        ),
        err
    )]
    async fn sync_oauth_scopes(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        provider_kind: Option<IntegrationProviderKind>,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let integration_connections = self
            .fetch_all_integration_connections(
                executor,
                user_id,
                Some(IntegrationConnectionStatus::Validated),
                true,
            )
            .await?;

        for integration_connection in integration_connections {
            if let Some(provider_kind) = provider_kind
                && integration_connection.provider.kind() != provider_kind
            {
                continue;
            }

            let provider_kind = integration_connection.provider.kind();
            let provider_config_key =
                self.nango_provider_keys
                    .get(&provider_kind)
                    .context(format!(
                        "No Nango provider config key found for {provider_kind}"
                    ))?;

            let nango_connection = self
                .nango_service
                .get_connection(integration_connection.connection_id, provider_config_key)
                .await?;
            let Some(nango_connection) = nango_connection else {
                warn!(
                    "Unknown Nango connection {}, skipping OAuth scopes sync",
                    integration_connection.connection_id
                );
                continue;
            };

            info!(
                "Updating OAuth scopes for {provider_kind} integration connection {} for user {user_id}",
                integration_connection.id
            );
            self.repository
                .update_integration_connection_status(
                    executor,
                    integration_connection.id,
                    IntegrationConnectionStatus::Validated,
                    None,
                    Some(nango_connection.get_registered_oauth_scopes()?),
                    user_id,
                )
                .await?;
        }

        Ok(())
    }

    /// Migrate existing Nango-managed OAuth tokens to locally-managed OAuth credentials.
    ///
    /// For each integration connection whose provider has a `migration_url()`, this method:
    /// 1. Fetches the current access token from Nango
    /// 2. Calls the provider's migration endpoint to exchange for short-lived + refresh token
    /// 3. Encrypts and stores the new tokens locally
    ///
    /// Returns `(migrated_count, failed_count)`.
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(provider_kind = provider_kind.map(|kind| kind.to_string())),
        err
    )]
    pub async fn migrate_nango_tokens(
        &self,
        provider_kind: Option<IntegrationProviderKind>,
        dry_run: bool,
    ) -> Result<(usize, usize), UniversalInboxError> {
        let token_encryption_key = self
            .token_encryption_key
            .as_ref()
            .map(|k| k.expose_secret())
            .ok_or_else(|| {
                UniversalInboxError::Unexpected(anyhow!(
                    "Token encryption key not configured but required for OAuth token migration"
                ))
            })?;

        let oauth2_flow_service = self.oauth2_flow_service.as_ref().ok_or_else(|| {
            UniversalInboxError::Unexpected(anyhow!(
                "OAuth2 flow service not configured but required for OAuth token migration"
            ))
        })?;

        // Collect providers that support migration
        let migratable_providers: Vec<_> = self
            .oauth2_providers
            .iter()
            .filter(|(kind, provider)| {
                provider.migration_url().is_some() && provider_kind.is_none_or(|pk| pk == **kind)
            })
            .collect();

        if migratable_providers.is_empty() {
            info!("No providers with migration support found");
            return Ok((0, 0));
        }

        info!(
            "Found {} provider(s) supporting token migration: {:?}",
            migratable_providers.len(),
            migratable_providers
                .iter()
                .map(|(k, _)| k.to_string())
                .collect::<Vec<_>>()
        );

        let service = self.user_service.clone();
        let mut user_transaction = service
            .begin()
            .await
            .context("Failed to create new transaction while listing users for migration")?;
        let users = service.fetch_all_users(&mut user_transaction).await?;
        drop(user_transaction);

        let mut migrated_count: usize = 0;
        let mut failed_count: usize = 0;

        for user in &users {
            let mut list_transaction = self.begin().await.context(format!(
                "Failed to create new transaction while listing connections for user {}",
                user.id
            ))?;

            let integration_connections = self
                .fetch_all_integration_connections(
                    &mut list_transaction,
                    user.id,
                    Some(IntegrationConnectionStatus::Validated),
                    false,
                )
                .await?;
            drop(list_transaction);

            for integration_connection in integration_connections {
                let ic_provider_kind = integration_connection.provider.kind();

                // Skip if this provider doesn't support migration
                let Some(oauth2_provider) = self.oauth2_providers.get(&ic_provider_kind) else {
                    continue;
                };
                if oauth2_provider.migration_url().is_none() {
                    continue;
                }
                // Skip if filtering by provider_kind and this doesn't match
                if provider_kind.is_some() && provider_kind != Some(ic_provider_kind) {
                    continue;
                }

                let nango_provider_key = match self.nango_provider_keys.get(&ic_provider_kind) {
                    Some(key) => key,
                    None => {
                        warn!(
                            "No Nango provider config key found for {ic_provider_kind}, skipping connection {}",
                            integration_connection.id
                        );
                        failed_count += 1;
                        continue;
                    }
                };

                if dry_run {
                    info!(
                        "[DRY RUN] Would migrate {ic_provider_kind} token for connection {} (user {})",
                        integration_connection.id, user.id
                    );
                    migrated_count += 1;
                    continue;
                }

                info!(
                    "Migrating {ic_provider_kind} token for connection {} (user {})",
                    integration_connection.id, user.id
                );

                // Step 1: Fetch current access token from Nango
                let nango_connection = match self
                    .nango_service
                    .get_connection(integration_connection.connection_id, nango_provider_key)
                    .await
                {
                    Ok(Some(conn)) => conn,
                    Ok(None) => {
                        warn!(
                            "No Nango connection found for connection {} (user {}), skipping",
                            integration_connection.id, user.id
                        );
                        failed_count += 1;
                        continue;
                    }
                    Err(err) => {
                        error!(
                            "Failed to fetch Nango connection for {} (user {}): {err:?}",
                            integration_connection.id, user.id
                        );
                        failed_count += 1;
                        continue;
                    }
                };

                // Step 2: Call migration endpoint
                let token_response = match oauth2_flow_service
                    .migrate_old_token(
                        oauth2_provider.as_ref(),
                        &nango_connection.credentials.access_token,
                    )
                    .await
                {
                    Ok(response) => response,
                    Err(err) => {
                        error!(
                            "Failed to migrate token for connection {} (user {}): {err:?}",
                            integration_connection.id, user.id
                        );
                        failed_count += 1;
                        continue;
                    }
                };

                // Step 3: Encrypt and store (bind ciphertext to this connection via AAD)
                let aad_context = integration_connection.id.0.as_bytes();
                let encrypted_access_token = match encrypt_token(
                    token_response.access_token.expose_secret().as_str(),
                    aad_context,
                    token_encryption_key,
                ) {
                    Ok(token) => token,
                    Err(err) => {
                        error!(
                            "Failed to encrypt access token for connection {} (user {}): {err:?}",
                            integration_connection.id, user.id
                        );
                        failed_count += 1;
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
                    Ok(token) => token,
                    Err(err) => {
                        error!(
                            "Failed to encrypt refresh token for connection {} (user {}): {err:?}",
                            integration_connection.id, user.id
                        );
                        failed_count += 1;
                        continue;
                    }
                };

                let raw_response = serde_json::to_value(token_response.as_safe_token_response())
                    .unwrap_or(serde_json::Value::Null);

                let mut store_transaction = self.begin().await.context(format!(
                    "Failed to create transaction for connection {} (user {})",
                    integration_connection.id, user.id
                ))?;

                match self
                    .repository
                    .store_oauth_credential(
                        &mut store_transaction,
                        integration_connection.id,
                        encrypted_access_token,
                        encrypted_refresh_token,
                        token_response.expires_at(),
                        raw_response,
                    )
                    .await
                {
                    Ok(_) => {
                        if let Err(err) = store_transaction.commit().await {
                            error!(
                                "Failed to commit migrated credential for connection {} (user {}): {err:?}",
                                integration_connection.id, user.id
                            );
                            failed_count += 1;
                        } else {
                            info!(
                                "Successfully migrated token for {ic_provider_kind} connection {} (user {})",
                                integration_connection.id, user.id
                            );
                            migrated_count += 1;
                        }
                    }
                    Err(err) => {
                        error!(
                            "Failed to store migrated credential for connection {} (user {}): {err:?}",
                            integration_connection.id, user.id
                        );
                        failed_count += 1;
                    }
                }
            }
        }

        info!("Token migration summary: {migrated_count} migrated, {failed_count} failed");
        Ok((migrated_count, failed_count))
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            provider_kind = provider_kind.to_string(),
            provider_user_id
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
        fields(integration_connection_id, user_id, status),
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
        skip(self, executor),
        fields(minutes_before_expiry, provider_kind),
        err
    )]
    pub async fn refresh_expiring_tokens(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        minutes_before_expiry: i64,
        provider_kind: Option<IntegrationProviderKind>,
    ) -> Result<(usize, usize), UniversalInboxError> {
        let token_encryption_key = self
            .token_encryption_key
            .as_ref()
            .map(|k| k.expose_secret())
            .ok_or_else(|| {
                UniversalInboxError::Unexpected(anyhow!(
                    "Token encryption key is not configured, cannot refresh tokens"
                ))
            })?;
        let flow_service = self.oauth2_flow_service.as_ref().ok_or_else(|| {
            UniversalInboxError::Unexpected(anyhow!(
                "OAuth2 flow service is not configured, cannot refresh tokens"
            ))
        })?;

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
            let raw_response =
                serde_json::to_value(token_response.as_safe_token_response()).unwrap_or_default();

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

        let provider_kind = integration_connection.provider.kind();

        // Look up the OAuth2Provider for this provider kind
        let provider = self.get_oauth2_provider(&provider_kind).ok_or_else(|| {
            UniversalInboxError::UnsupportedAction(format!(
                "No OAuth2 provider configured for {provider_kind:?}"
            ))
        })?;

        let oauth2_flow_service = self.oauth2_flow_service.as_ref().ok_or_else(|| {
            UniversalInboxError::Unexpected(anyhow::anyhow!("OAuth2 flow service not configured"))
        })?;

        let redirect_uri = oauth2_flow_service.redirect_uri();

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

        // Add scopes as an extra param (some providers use non-standard separators)
        let scopes = provider.required_scopes();
        if !scopes.is_empty() {
            auth_request = auth_request.add_extra_param("scope", scopes.join(","));
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

    pub async fn complete_oauth_callback(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        authorization_code: &SecretBox<AuthorizationCode>,
        state: &str,
        cache: &Cache,
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

        let provider = self
            .get_oauth2_provider(&state_data.provider_kind)
            .ok_or_else(|| {
                UniversalInboxError::Unexpected(anyhow::anyhow!(
                    "No OAuth2 provider configured for {:?}",
                    state_data.provider_kind
                ))
            })?;

        let oauth2_flow_service = self.oauth2_flow_service.as_ref().ok_or_else(|| {
            UniversalInboxError::Unexpected(anyhow::anyhow!("OAuth2 flow service not configured"))
        })?;

        let token_encryption_key = self
            .token_encryption_key
            .as_ref()
            .map(|k| k.expose_secret())
            .ok_or_else(|| {
                UniversalInboxError::Unexpected(anyhow::anyhow!(
                    "Token encryption key not configured"
                ))
            })?;

        let token_response = oauth2_flow_service
            .exchange_code_for_token(
                provider,
                authorization_code,
                state_data.pkce_verifier.as_ref(),
            )
            .await?;

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

        let raw_response = serde_json::to_value(token_response.as_safe_token_response())
            .context("Failed to serialize token response to Value")?;
        let registered_scopes = provider.extract_registered_scopes(&raw_response)?;

        // Get the integration connection and verify it is still in Created status.
        // This prevents a late callback from a duplicate authorize flow from overwriting
        // credentials stored by an earlier successful callback.
        let integration_connection = self
            .get_integration_connection(executor, state_data.integration_connection_id)
            .await?
            .ok_or_else(|| {
                UniversalInboxError::Unexpected(anyhow::anyhow!(
                    "Integration connection {} not found",
                    state_data.integration_connection_id
                ))
            })?;

        if integration_connection.status != IntegrationConnectionStatus::Created {
            return Err(UniversalInboxError::UnsupportedAction(format!(
                "Integration connection {} is no longer in Created status (current: {:?}), ignoring stale OAuth callback",
                state_data.integration_connection_id, integration_connection.status
            )));
        }

        self.repository
            .store_oauth_credential(
                executor,
                state_data.integration_connection_id,
                encrypted_access_token,
                encrypted_refresh_token,
                expires_at,
                raw_response,
            )
            .await?;

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

#[io_cached(
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

    Ok(integration_connection.map(|connection| connection.provider.config()))
}
