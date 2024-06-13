use std::{collections::HashMap, sync::Arc};

use anyhow::{anyhow, Context};
use chrono::{TimeDelta, Utc};
use sqlx::{Postgres, Transaction};
use tracing::{error, info, warn};

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        provider::{IntegrationConnectionContext, IntegrationProviderKind},
        IntegrationConnection, IntegrationConnectionId, IntegrationConnectionStatus,
        NangoProviderKey,
    },
    user::UserId,
};

use crate::{
    integrations::oauth2::{AccessToken, NangoService},
    repository::Repository,
    repository::{
        integration_connection::IntegrationConnectionRepository,
        notification::NotificationRepository,
    },
    universal_inbox::{user::service::UserService, UniversalInboxError, UpdateStatus},
};

pub struct IntegrationConnectionService {
    repository: Arc<Repository>,
    nango_service: NangoService,
    nango_provider_keys: HashMap<IntegrationProviderKind, NangoProviderKey>,
    user_service: Arc<UserService>,
}

pub const UNKNOWN_NANGO_CONNECTION_ERROR_MESSAGE: &str = "🔌 The OAuth connection is failing due to a technical issue on our end. Please try to reconnect the integration. If the issue keeps happening, please contact our support.";

impl IntegrationConnectionService {
    pub fn new(
        repository: Arc<Repository>,
        nango_service: NangoService,
        nango_provider_keys: HashMap<IntegrationProviderKind, NangoProviderKey>,
        user_service: Arc<UserService>,
    ) -> IntegrationConnectionService {
        IntegrationConnectionService {
            repository,
            nango_service,
            nango_provider_keys,
            user_service,
        }
    }

    pub async fn begin(&self) -> Result<Transaction<Postgres>, UniversalInboxError> {
        self.repository.begin().await
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    pub async fn fetch_all_integration_connections<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        for_user_id: UserId,
        status: Option<IntegrationConnectionStatus>,
    ) -> Result<Vec<IntegrationConnection>, UniversalInboxError> {
        self.repository
            .fetch_all_integration_connections(executor, for_user_id, status)
            .await
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    pub async fn create_integration_connection<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
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

    #[tracing::instrument(level = "debug", skip(self, executor))]
    pub async fn update_integration_connection_config<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
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
                .delete_notification_details(executor, kind)
                .await?;
            self.repository.delete_notifications(executor, kind).await?;
            self.repository
                .update_integration_connection_context(executor, integration_connection_id, None)
                .await?;
        }

        Ok(updated_integration_connection_config)
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    pub async fn verify_integration_connection<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
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
        self.repository
            .update_integration_connection_status(
                executor,
                integration_connection_id,
                IntegrationConnectionStatus::Validated,
                None,
                Some(nango_connection.get_registered_oauth_scopes()),
                for_user_id,
            )
            .await
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    pub async fn disconnect_integration_connection<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
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

    #[tracing::instrument(level = "debug", skip(self, executor))]
    pub async fn get_integration_connection_to_sync<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        integration_provider_kind: IntegrationProviderKind,
        min_sync_interval_in_minutes: i64,
        for_user_id: UserId,
    ) -> Result<Option<IntegrationConnection>, UniversalInboxError> {
        self
            .repository
            .get_integration_connection_per_provider(
                executor,
                for_user_id,
                integration_provider_kind,
                Some(
                    Utc::now()
                        - TimeDelta::try_minutes(min_sync_interval_in_minutes)
                        .unwrap_or_else(|| {
                            panic!(
                                "Invalid `min_sync_interval_in_minutes` value: {min_sync_interval_in_minutes}"
                            )
                        }),
                ),
                Some(IntegrationConnectionStatus::Validated),
            )
            .await
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    pub async fn find_access_token<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
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

        if let Some(integration_connection) = integration_connection {
            let provider_kind = integration_connection.provider.kind();
            let provider_config_key =
                self.nango_provider_keys
                    .get(&provider_kind)
                    .context(format!(
                        "No Nango provider config key found for {provider_kind}"
                    ))?;

            if let Some(nango_connection) = self
                .nango_service
                .get_connection(integration_connection.connection_id, provider_config_key)
                .await?
            {
                if integration_provider_kind == IntegrationProviderKind::Slack {
                    if let Some(access_token) =
                        nango_connection.credentials.raw["authed_user"]["access_token"].as_str()
                    {
                        return Ok(Some((
                            AccessToken(access_token.to_string()),
                            integration_connection,
                        )));
                    }
                }
                return Ok(Some((
                    nango_connection.credentials.access_token,
                    integration_connection,
                )));
            }

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

            return Err(UniversalInboxError::Recoverable(anyhow!(
                "Unknown Nango connection: {}",
                integration_connection.connection_id
            )));
        }

        Ok(None)
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    pub async fn update_integration_connection_context<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
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

    #[tracing::instrument(level = "debug", skip(self, executor))]
    pub async fn get_integration_connection_per_provider_user_id<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
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

    #[tracing::instrument(level = "debug", skip(self, executor))]
    pub async fn start_sync_status<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        integration_provider_kind: IntegrationProviderKind,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError> {
        self.repository
            .update_integration_connection_sync_status(
                executor,
                for_user_id,
                integration_provider_kind,
                None,
                true,
            )
            .await
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    pub async fn error_sync_status<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        integration_provider_kind: IntegrationProviderKind,
        failure_message: String,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError> {
        self.repository
            .update_integration_connection_sync_status(
                executor,
                for_user_id,
                integration_provider_kind,
                Some(failure_message),
                false,
            )
            .await
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    pub async fn reset_error_sync_status<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        integration_provider_kind: IntegrationProviderKind,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError> {
        self.repository
            .update_integration_connection_sync_status(
                executor,
                for_user_id,
                integration_provider_kind,
                None,
                false,
            )
            .await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn sync_oauth_scopes_for_all_users<'a>(
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

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn sync_oauth_scopes_for_user<'a>(
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

    #[tracing::instrument(level = "debug", skip(self))]
    async fn sync_oauth_scopes<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        provider_kind: Option<IntegrationProviderKind>,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let integration_connections = self
            .fetch_all_integration_connections(
                executor,
                user_id,
                Some(IntegrationConnectionStatus::Validated),
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
                    Some(nango_connection.get_registered_oauth_scopes()),
                    user_id,
                )
                .await?;
        }

        Ok(())
    }
}
