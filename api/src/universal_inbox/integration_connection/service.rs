use std::{collections::HashMap, sync::Arc};

use anyhow::Context;
use chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionId, IntegrationConnectionStatus,
        IntegrationProviderKind, NangoProviderKey,
    },
    user::UserId,
};

use crate::{
    integrations::oauth2::{AccessToken, NangoService},
    repository::integration_connection::IntegrationConnectionRepository,
    repository::Repository,
    universal_inbox::{UniversalInboxError, UpdateStatus},
};

#[derive(Debug)]
pub struct IntegrationConnectionService {
    repository: Arc<Repository>,
    nango_service: NangoService,
    nango_provider_keys: HashMap<IntegrationProviderKind, NangoProviderKey>,
}

pub const UNKNOWN_NANGO_CONNECTION_ERROR_MESSAGE: &str = "🔌 The OAuth connection is failing due to a technical issue on our end. Please try to reconnect the integration. If the issue keeps happening, please contact our support.";

impl IntegrationConnectionService {
    pub fn new(
        repository: Arc<Repository>,
        nango_service: NangoService,
        nango_provider_keys: HashMap<IntegrationProviderKind, NangoProviderKey>,
    ) -> IntegrationConnectionService {
        IntegrationConnectionService {
            repository,
            nango_service,
            nango_provider_keys,
        }
    }

    pub async fn begin(&self) -> Result<Transaction<Postgres>, UniversalInboxError> {
        self.repository.begin().await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn fetch_all_integration_connections<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        for_user_id: UserId,
    ) -> Result<Vec<IntegrationConnection>, UniversalInboxError> {
        self.repository
            .fetch_all_integration_connections(executor, for_user_id)
            .await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn create_integration_connection<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        integration_provider_kind: IntegrationProviderKind,
        for_user_id: UserId,
    ) -> Result<Box<IntegrationConnection>, UniversalInboxError> {
        let integration_connection = Box::new(IntegrationConnection::new(
            for_user_id,
            integration_provider_kind,
        ));

        self.repository
            .create_integration_connection(executor, integration_connection)
            .await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn verify_integration_connection<'a>(
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

            let provider_config_key = self
                .nango_provider_keys
                .get(&integration_connection.provider_kind)
                .context(format!(
                    "No Nango provider config key found for {}",
                    integration_connection.provider_kind,
                ))?;

            let nango_connection_exists = self
                .nango_service
                .get_connection(integration_connection.connection_id, provider_config_key)
                .await?
                .is_some();
            let new_status = if nango_connection_exists {
                IntegrationConnectionStatus::Validated
            } else {
                IntegrationConnectionStatus::Failing
            };
            let failure_message = (!nango_connection_exists)
                .then(|| UNKNOWN_NANGO_CONNECTION_ERROR_MESSAGE.to_string());

            return self
                .repository
                .update_integration_connection_status(
                    executor,
                    integration_connection_id,
                    new_status,
                    failure_message,
                    for_user_id,
                )
                .await;
        }

        Ok(UpdateStatus {
            updated: false,
            result: None,
        })
    }

    #[tracing::instrument(level = "debug", skip(self))]
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

            let provider_config_key = self
                .nango_provider_keys
                .get(&integration_connection.provider_kind)
                .context(format!(
                    "No Nango provider config key found for {}",
                    integration_connection.provider_kind,
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
                    for_user_id,
                )
                .await;
        }

        Ok(UpdateStatus {
            updated: false,
            result: None,
        })
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn get_access_token<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        integration_provider_kind: IntegrationProviderKind,
        synced_before: Option<DateTime<Utc>>,
        for_user_id: UserId,
    ) -> Result<Option<AccessToken>, UniversalInboxError> {
        let integration_connection = self
            .repository
            .get_integration_connection_per_provider(
                executor,
                for_user_id,
                integration_provider_kind,
                synced_before,
            )
            .await?;

        if let Some(integration_connection) = integration_connection {
            let provider_config_key = self
                .nango_provider_keys
                .get(&integration_connection.provider_kind)
                .context(format!(
                    "No Nango provider config key found for {}",
                    integration_connection.provider_kind,
                ))?;

            if let Some(nango_connection) = self
                .nango_service
                .get_connection(integration_connection.connection_id, provider_config_key)
                .await?
            {
                return Ok(Some(nango_connection.credentials.access_token));
            }

            self.repository
                .update_integration_connection_status(
                    executor,
                    integration_connection.id,
                    IntegrationConnectionStatus::Failing,
                    Some(UNKNOWN_NANGO_CONNECTION_ERROR_MESSAGE.to_string()),
                    for_user_id,
                )
                .await?;

            return Err(UniversalInboxError::UnknownNangoConnectionError(
                integration_connection.connection_id,
            ));
        }

        Ok(None)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn update_integration_connection_sync_status<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        user_id: UserId,
        integration_provider_kind: IntegrationProviderKind,
        failure_message: Option<String>,
    ) -> Result<UpdateStatus<Box<IntegrationConnection>>, UniversalInboxError> {
        self.repository
            .update_integration_connection_sync_status(
                executor,
                user_id,
                integration_provider_kind,
                failure_message,
            )
            .await
    }
}
