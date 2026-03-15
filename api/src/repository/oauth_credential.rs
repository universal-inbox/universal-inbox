use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use universal_inbox::{
    integration_connection::IntegrationConnectionId,
    integration_connection::provider::IntegrationProviderKind,
};

use crate::{repository::Repository, universal_inbox::UniversalInboxError};

/// A stored OAuth credential with encrypted tokens.
/// The tokens are stored as encrypted byte arrays and must be decrypted
/// by the service layer before use.
#[derive(Debug, Clone)]
pub struct StoredOAuthCredential {
    pub integration_connection_id: IntegrationConnectionId,
    pub encrypted_access_token: Vec<u8>,
    pub encrypted_refresh_token: Option<Vec<u8>>,
    pub access_token_expires_at: Option<DateTime<Utc>>,
    pub raw_token_response: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Minimal info needed for the eager token refresh command.
#[derive(Debug, Clone)]
pub struct ExpiringOAuthCredential {
    pub integration_connection_id: IntegrationConnectionId,
    pub encrypted_refresh_token: Vec<u8>,
    pub provider_kind: IntegrationProviderKind,
}

#[async_trait]
pub trait OAuthCredentialRepository {
    async fn store_oauth_credential(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        encrypted_access_token: Vec<u8>,
        encrypted_refresh_token: Option<Vec<u8>>,
        access_token_expires_at: Option<DateTime<Utc>>,
        raw_token_response: serde_json::Value,
    ) -> Result<StoredOAuthCredential, UniversalInboxError>;

    async fn get_oauth_credential(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
    ) -> Result<Option<StoredOAuthCredential>, UniversalInboxError>;

    async fn delete_oauth_credential(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
    ) -> Result<(), UniversalInboxError>;

    async fn list_expiring_credentials(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        expiring_before: DateTime<Utc>,
        provider_kind: Option<IntegrationProviderKind>,
    ) -> Result<Vec<ExpiringOAuthCredential>, UniversalInboxError>;
}

#[async_trait]
impl OAuthCredentialRepository for Repository {
    #[tracing::instrument(level = "debug", skip_all, err)]
    async fn store_oauth_credential(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        encrypted_access_token: Vec<u8>,
        encrypted_refresh_token: Option<Vec<u8>>,
        access_token_expires_at: Option<DateTime<Utc>>,
        raw_token_response: serde_json::Value,
    ) -> Result<StoredOAuthCredential, UniversalInboxError> {
        let now = Utc::now();
        let row = sqlx::query!(
            r#"
                INSERT INTO oauth_credential
                  (integration_connection_id, encrypted_access_token, encrypted_refresh_token,
                   access_token_expires_at, raw_token_response, created_at, updated_at)
                VALUES
                  ($1, $2, $3, $4, $5, $6, $7)
                ON CONFLICT (integration_connection_id) DO UPDATE SET
                  encrypted_access_token = EXCLUDED.encrypted_access_token,
                  encrypted_refresh_token = COALESCE(EXCLUDED.encrypted_refresh_token, oauth_credential.encrypted_refresh_token),
                  access_token_expires_at = EXCLUDED.access_token_expires_at,
                  raw_token_response = EXCLUDED.raw_token_response,
                  updated_at = EXCLUDED.updated_at
                RETURNING
                  integration_connection_id,
                  encrypted_access_token,
                  encrypted_refresh_token,
                  access_token_expires_at,
                  raw_token_response,
                  created_at,
                  updated_at
            "#,
            Uuid::from(integration_connection_id),
            &encrypted_access_token,
            encrypted_refresh_token.as_deref(),
            access_token_expires_at,
            &raw_token_response,
            now,
            now,
        )
        .fetch_one(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!(
                "Failed to store OAuth credential for integration connection {integration_connection_id}: {err}"
            );
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        Ok(StoredOAuthCredential {
            integration_connection_id: IntegrationConnectionId(row.integration_connection_id),
            encrypted_access_token: row.encrypted_access_token,
            encrypted_refresh_token: row.encrypted_refresh_token,
            access_token_expires_at: row.access_token_expires_at,
            raw_token_response: row.raw_token_response,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    async fn get_oauth_credential(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
    ) -> Result<Option<StoredOAuthCredential>, UniversalInboxError> {
        let row = sqlx::query!(
            r#"
                SELECT
                  integration_connection_id,
                  encrypted_access_token,
                  encrypted_refresh_token,
                  access_token_expires_at,
                  raw_token_response,
                  created_at,
                  updated_at
                FROM oauth_credential
                WHERE integration_connection_id = $1
            "#,
            Uuid::from(integration_connection_id),
        )
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!(
                "Failed to fetch OAuth credential for integration connection {integration_connection_id}: {err}"
            );
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        Ok(row.map(|row| StoredOAuthCredential {
            integration_connection_id: IntegrationConnectionId(row.integration_connection_id),
            encrypted_access_token: row.encrypted_access_token,
            encrypted_refresh_token: row.encrypted_refresh_token,
            access_token_expires_at: row.access_token_expires_at,
            raw_token_response: row.raw_token_response,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }))
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    async fn delete_oauth_credential(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
    ) -> Result<(), UniversalInboxError> {
        sqlx::query!(
            r#"
                DELETE FROM oauth_credential
                WHERE integration_connection_id = $1
            "#,
            Uuid::from(integration_connection_id),
        )
        .execute(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!(
                "Failed to delete OAuth credential for integration connection {integration_connection_id}: {err}"
            );
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    async fn list_expiring_credentials(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        expiring_before: DateTime<Utc>,
        provider_kind: Option<IntegrationProviderKind>,
    ) -> Result<Vec<ExpiringOAuthCredential>, UniversalInboxError> {
        let provider_kind_str = provider_kind.map(|pk| pk.to_string());

        let rows = sqlx::query!(
            r#"
                SELECT
                  oc.integration_connection_id,
                  oc.encrypted_refresh_token,
                  ic.provider_kind AS "provider_kind: String"
                FROM oauth_credential oc
                JOIN integration_connection ic ON ic.id = oc.integration_connection_id
                WHERE oc.encrypted_refresh_token IS NOT NULL
                  AND oc.access_token_expires_at IS NOT NULL
                  AND oc.access_token_expires_at < $1
                  AND ic.status = 'Validated'
                  AND ($2::TEXT IS NULL OR ic.provider_kind::TEXT = $2)
                FOR UPDATE OF oc SKIP LOCKED
            "#,
            expiring_before,
            provider_kind_str,
        )
        .fetch_all(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!("Failed to list expiring OAuth credentials: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        rows.into_iter()
            .map(|row| {
                let provider_kind: IntegrationProviderKind =
                    row.provider_kind.parse().map_err(|_| {
                        UniversalInboxError::Unexpected(anyhow::anyhow!(
                            "Unknown provider kind: {}",
                            row.provider_kind
                        ))
                    })?;
                Ok(ExpiringOAuthCredential {
                    integration_connection_id: IntegrationConnectionId(
                        row.integration_connection_id,
                    ),
                    encrypted_refresh_token: row.encrypted_refresh_token.ok_or_else(|| {
                        UniversalInboxError::Unexpected(anyhow::anyhow!(
                            "Missing refresh token for credential {}",
                            row.integration_connection_id
                        ))
                    })?,
                    provider_kind,
                })
            })
            .collect()
    }
}
