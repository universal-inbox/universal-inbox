use core::fmt;
use std::{collections::HashMap, sync::Arc};

use anyhow::{anyhow, Context};
use apalis::{prelude::*, redis::RedisStorage};
use cached::proc_macro::io_cached;
use chrono::{TimeDelta, Utc};
use sqlx::{Postgres, Transaction};
use tokio_retry::{
    strategy::{jitter, ExponentialBackoff},
    Retry,
};
use tracing::{error, info, warn};

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        provider::{IntegrationConnectionContext, IntegrationProviderKind},
        IntegrationConnection, IntegrationConnectionId, IntegrationConnectionStatus,
        NangoProviderKey,
    },
    notification::NotificationSyncSourceKind,
    task::TaskSyncSourceKind,
    user::UserId,
};

use crate::{
    integrations::oauth2::{AccessToken, NangoService},
    jobs::{
        sync::{SyncNotificationsJob, SyncTasksJob},
        UniversalInboxJob, UniversalInboxJobPayload,
    },
    repository::{
        integration_connection::{
            IntegrationConnectionRepository, IntegrationConnectionSyncStatusUpdate,
            IntegrationConnectionSyncedBeforeFilter,
        },
        notification::NotificationRepository,
        Repository,
    },
    universal_inbox::{user::service::UserService, UniversalInboxError, UpdateStatus},
    utils::cache::build_redis_cache,
};

pub struct IntegrationConnectionService {
    repository: Arc<Repository>,
    nango_service: NangoService,
    nango_provider_keys: HashMap<IntegrationProviderKind, NangoProviderKey>,
    user_service: Arc<UserService>,
    min_sync_notifications_interval_in_minutes: i64,
    min_sync_tasks_interval_in_minutes: i64,
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
    pub fn new(
        repository: Arc<Repository>,
        nango_service: NangoService,
        nango_provider_keys: HashMap<IntegrationProviderKind, NangoProviderKey>,
        user_service: Arc<UserService>,
        min_sync_notifications_interval_in_minutes: i64,
        min_sync_tasks_interval_in_minutes: i64,
    ) -> IntegrationConnectionService {
        IntegrationConnectionService {
            repository,
            nango_service,
            nango_provider_keys,
            user_service,
            min_sync_notifications_interval_in_minutes,
            min_sync_tasks_interval_in_minutes,
        }
    }

    pub async fn begin(&self) -> Result<Transaction<'_, Postgres>, UniversalInboxError> {
        self.repository.begin().await
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
                true,
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
        info!("Triggering sync notifications job for {notification_sync_source_kind:?} integration connection for user {for_user_id:?}");
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
                    .push(UniversalInboxJob::new(
                        UniversalInboxJobPayload::SyncNotifications(SyncNotificationsJob {
                            source: notification_sync_source_kind,
                            user_id: for_user_id,
                        }),
                    ))
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
        info!("Triggering sync tasks job for {task_sync_source_kind:?} integration connection for user {for_user_id:?}");
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
                    .push(UniversalInboxJob::new(UniversalInboxJobPayload::SyncTasks(
                        SyncTasksJob {
                            source: task_sync_source_kind,
                            user_id: for_user_id,
                        },
                    )))
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
            integration_provider_kind = integration_provider_kind.to_string()
        ),
        err
    )]
    pub async fn create_integration_connection(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_provider_kind: IntegrationProviderKind,
        for_user_id: UserId,
    ) -> Result<Box<IntegrationConnection>, UniversalInboxError> {
        let integration_connection = Box::new(IntegrationConnection::new(
            for_user_id,
            integration_provider_kind.default_integration_connection_config(),
        ));

        self.repository
            .create_integration_connection(executor, integration_connection)
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
            return Err(UniversalInboxError::Forbidden(format!("Only the owner of the integration connection {integration_connection_id} can verify it")));
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
                return Err(UniversalInboxError::Forbidden(format!("Only the owner of the integration connection {integration_connection_id} can verify it")));
            }

            let provider_kind = integration_connection.provider.kind();
            let provider_config_key =
                self.nango_provider_keys
                    .get(&provider_kind)
                    .context(format!(
                        "No Nango provider config key found for {provider_kind}"
                    ))?;

            self.nango_service
                .delete_connection(integration_connection.connection_id, provider_config_key)
                .await?;

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
        self.repository
            .get_integration_connection_per_provider(
                executor,
                for_user_id,
                integration_provider_kind,
                synced_before_filter,
                Some(IntegrationConnectionStatus::Validated),
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

    /// This function randomly search for a validated Slack integration connection to access
    /// Slack API endpoint not related to a specific user.
    #[tracing::instrument(level = "debug", skip(self, executor), err)]
    pub async fn find_slack_access_token(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        context: IntegrationConnectionContext,
    ) -> Result<Option<(AccessToken, IntegrationConnection)>, UniversalInboxError> {
        let integration_connection = self
            .repository
            .get_integration_connection_per_context(executor, context)
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

        self.fetch_access_token_from_nango(executor, integration_connection, Some(for_user_id))
            .await
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
        if integration_connection.provider.context_is_empty() {
            if let Some(provider_context) = nango_connection.get_provider_context() {
                self.repository
                    .update_integration_connection_context(
                        executor,
                        integration_connection.id,
                        Some(provider_context),
                    )
                    .await?;
            }
        }

        if provider_kind == IntegrationProviderKind::Slack {
            if let Some(access_token) =
                nango_connection.credentials.raw["authed_user"]["access_token"].as_str()
            {
                return Ok(Some((
                    AccessToken(access_token.to_string()),
                    integration_connection,
                )));
            }
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
            if let Some(provider_kind) = provider_kind {
                if integration_connection.provider.kind() != provider_kind {
                    continue;
                }
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
}

#[io_cached(
    key = "String",
    convert = r#"{ format!("{}{}", provider_kind, provider_user_id) }"#,
    ty = "cached::AsyncRedisCache<String, Option<IntegrationConnectionConfig>>",
    map_error = r##"|e| UniversalInboxError::Unexpected(anyhow!("Failed to cache Slack `is_known_provider_user_id`: {:?}", e))"##,
    create = r##" { build_redis_cache("slack:is_known_provider_user_id", 86400, false).await }"##
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
