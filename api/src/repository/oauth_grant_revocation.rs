use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use universal_inbox::integration_connection::{
    IntegrationConnectionId,
    provider::{IntegrationConnectionContext, IntegrationProviderKind},
};

use crate::observability::attr;
use crate::{
    repository::Repository,
    universal_inbox::UniversalInboxError,
    utils::crypto::{aad, data_keyring},
};

/// An OAuth grant to revoke at its provider. The tokens are encrypted with the
/// row id as AAD, so they outlive the integration connection.
#[derive(Debug, Clone)]
pub struct NewOAuthGrantRevocation {
    pub id: Uuid,
    pub provider_kind: IntegrationProviderKind,
    pub integration_connection_id: Option<IntegrationConnectionId>,
    pub provider_user_id: Option<String>,
    pub provider_context: Option<IntegrationConnectionContext>,
    pub encrypted_access_token: Vec<u8>,
    pub encrypted_refresh_token: Option<Vec<u8>>,
    pub access_token_expires_at: Option<DateTime<Utc>>,
    pub last_error: String,
    pub next_attempt_at: DateTime<Utc>,
}

/// A Pending revocation due for a retry, locked for the claiming transaction.
#[derive(Debug, Clone)]
pub struct PendingOAuthGrantRevocation {
    pub id: Uuid,
    pub provider_kind: IntegrationProviderKind,
    pub integration_connection_id: Option<IntegrationConnectionId>,
    pub provider_user_id: Option<String>,
    pub encrypted_access_token: Vec<u8>,
    pub encrypted_refresh_token: Option<Vec<u8>>,
    pub access_token_expires_at: Option<DateTime<Utc>>,
    pub attempts: u32,
}

#[async_trait]
pub trait OAuthGrantRevocationRepository {
    /// Record a grant whose inline revocation failed. `attempts` starts at 1
    /// to account for that inline attempt.
    async fn create_oauth_grant_revocation(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        revocation: NewOAuthGrantRevocation,
    ) -> Result<(), UniversalInboxError>;

    /// Lock and return the oldest Pending revocation due before `now`.
    /// `FOR UPDATE SKIP LOCKED` lets concurrent workers each claim a
    /// different row; the lock lasts until the transaction ends.
    async fn claim_due_oauth_grant_revocation(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        now: DateTime<Utc>,
    ) -> Result<Option<PendingOAuthGrantRevocation>, UniversalInboxError>;

    /// Mark a revocation Revoked (or Cancelled when `cancelled`), wiping its
    /// tokens.
    async fn complete_oauth_grant_revocation(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: Uuid,
        cancelled: bool,
        last_error: Option<String>,
    ) -> Result<(), UniversalInboxError>;

    /// Count a failed attempt and store the tokens to use for the next one
    /// (a refresh may have rotated them). `next_attempt_at = None` abandons
    /// the revocation.
    #[allow(clippy::too_many_arguments)]
    async fn record_oauth_grant_revocation_failure(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: Uuid,
        encrypted_access_token: Vec<u8>,
        encrypted_refresh_token: Option<Vec<u8>>,
        access_token_expires_at: Option<DateTime<Utc>>,
        last_error: String,
        next_attempt_at: Option<DateTime<Utc>>,
    ) -> Result<(), UniversalInboxError>;

    /// Whether a credential is stored again for the integration connection
    /// or for the same provider account, i.e. the grant was reconnected.
    async fn has_oauth_credential_for_provider_account(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        provider_kind: IntegrationProviderKind,
        integration_connection_id: Option<IntegrationConnectionId>,
        provider_user_id: Option<&str>,
    ) -> Result<bool, UniversalInboxError>;
}

#[async_trait]
impl OAuthGrantRevocationRepository for Repository {
    #[tracing::instrument(level = "debug", skip_all, fields({ attr::OAUTH_GRANT_REVOCATION_ID } = revocation.id.to_string()))]
    async fn create_oauth_grant_revocation(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        revocation: NewOAuthGrantRevocation,
    ) -> Result<(), UniversalInboxError> {
        sqlx::query!(
            r#"
                INSERT INTO oauth_grant_revocation
                  (id, provider_kind, integration_connection_id, provider_user_id,
                   provider_context_enc, encrypted_access_token, encrypted_refresh_token,
                   access_token_expires_at, attempts, last_error, next_attempt_at)
                VALUES
                  ($1, $2::integration_provider_kind, $3, $4, $5, $6, $7, $8, 1, $9, $10)
            "#,
            revocation.id,
            revocation.provider_kind.to_string() as _,
            revocation.integration_connection_id.map(Uuid::from),
            revocation.provider_user_id,
            revocation
                .provider_context
                .as_ref()
                .map(|context| data_keyring()?.encrypt_json(
                    context,
                    &aad("oauth_grant_revocation.provider_context", revocation.id)
                ))
                .transpose()?,
            &revocation.encrypted_access_token,
            revocation.encrypted_refresh_token.as_deref(),
            revocation.access_token_expires_at,
            revocation.last_error,
            revocation.next_attempt_at,
        )
        .execute(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!(
                "Failed to record the OAuth grant revocation {}: {err}",
                revocation.id
            );
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn claim_due_oauth_grant_revocation(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        now: DateTime<Utc>,
    ) -> Result<Option<PendingOAuthGrantRevocation>, UniversalInboxError> {
        let row = sqlx::query!(
            r#"
                SELECT
                  id,
                  provider_kind::TEXT AS "provider_kind!",
                  integration_connection_id,
                  provider_user_id,
                  encrypted_access_token AS "encrypted_access_token!",
                  encrypted_refresh_token,
                  access_token_expires_at,
                  attempts
                FROM oauth_grant_revocation
                WHERE status = 'Pending'
                  AND next_attempt_at <= $1
                  AND encrypted_access_token IS NOT NULL
                ORDER BY next_attempt_at
                LIMIT 1
                FOR UPDATE SKIP LOCKED
            "#,
            now,
        )
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| UniversalInboxError::DatabaseError {
            source: err,
            message: "Failed to claim a due OAuth grant revocation".to_string(),
        })?;

        row.map(|row| {
            let provider_kind: IntegrationProviderKind =
                row.provider_kind.parse().map_err(|_| {
                    UniversalInboxError::Unexpected(anyhow::anyhow!(
                        "Unknown provider kind: {}",
                        row.provider_kind
                    ))
                })?;
            Ok(PendingOAuthGrantRevocation {
                id: row.id,
                provider_kind,
                integration_connection_id: row
                    .integration_connection_id
                    .map(IntegrationConnectionId),
                provider_user_id: row.provider_user_id,
                encrypted_access_token: row.encrypted_access_token,
                encrypted_refresh_token: row.encrypted_refresh_token,
                access_token_expires_at: row.access_token_expires_at,
                attempts: row.attempts.try_into().unwrap_or_default(),
            })
        })
        .transpose()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::OAUTH_GRANT_REVOCATION_ID } = id.to_string(),
            { attr::OAUTH_GRANT_REVOCATION_CANCELLED } = cancelled
        )
    )]
    async fn complete_oauth_grant_revocation(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: Uuid,
        cancelled: bool,
        last_error: Option<String>,
    ) -> Result<(), UniversalInboxError> {
        let status = if cancelled { "Cancelled" } else { "Revoked" };
        sqlx::query!(
            r#"
                UPDATE oauth_grant_revocation
                SET status = $2::oauth_grant_revocation_status,
                    encrypted_access_token = NULL,
                    encrypted_refresh_token = NULL,
                    last_error = COALESCE($3, last_error),
                    completed_at = NOW(),
                    updated_at = NOW()
                WHERE id = $1
            "#,
            id,
            status as _,
            last_error,
        )
        .execute(&mut **executor)
        .await
        .map_err(|err| UniversalInboxError::DatabaseError {
            source: err,
            message: format!("Failed to complete the OAuth grant revocation {id}"),
        })?;

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all, fields({ attr::OAUTH_GRANT_REVOCATION_ID } = id.to_string()))]
    async fn record_oauth_grant_revocation_failure(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: Uuid,
        encrypted_access_token: Vec<u8>,
        encrypted_refresh_token: Option<Vec<u8>>,
        access_token_expires_at: Option<DateTime<Utc>>,
        last_error: String,
        next_attempt_at: Option<DateTime<Utc>>,
    ) -> Result<(), UniversalInboxError> {
        sqlx::query!(
            r#"
                UPDATE oauth_grant_revocation
                SET attempts = attempts + 1,
                    encrypted_access_token = $2,
                    encrypted_refresh_token = $3,
                    access_token_expires_at = $4,
                    last_error = $5,
                    next_attempt_at = COALESCE($6, next_attempt_at),
                    status = CASE WHEN $6::TIMESTAMPTZ IS NULL
                      THEN 'Abandoned'::oauth_grant_revocation_status
                      ELSE status
                    END,
                    completed_at = CASE WHEN $6::TIMESTAMPTZ IS NULL THEN NOW() ELSE NULL END,
                    updated_at = NOW()
                WHERE id = $1
            "#,
            id,
            &encrypted_access_token,
            encrypted_refresh_token.as_deref(),
            access_token_expires_at,
            last_error,
            next_attempt_at,
        )
        .execute(&mut **executor)
        .await
        .map_err(|err| UniversalInboxError::DatabaseError {
            source: err,
            message: format!(
                "Failed to record a failed attempt of the OAuth grant revocation {id}"
            ),
        })?;

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn has_oauth_credential_for_provider_account(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        provider_kind: IntegrationProviderKind,
        integration_connection_id: Option<IntegrationConnectionId>,
        provider_user_id: Option<&str>,
    ) -> Result<bool, UniversalInboxError> {
        let reconnected = sqlx::query_scalar!(
            r#"
                SELECT EXISTS (
                  SELECT 1
                  FROM oauth_credential oc
                  JOIN integration_connection ic ON ic.id = oc.integration_connection_id
                  WHERE ic.id = $2
                     OR (ic.provider_kind::TEXT = $1 AND ic.provider_user_id = $3)
                ) AS "reconnected!"
            "#,
            provider_kind.to_string(),
            integration_connection_id.map(Uuid::from),
            provider_user_id,
        )
        .fetch_one(&mut **executor)
        .await
        .map_err(|err| UniversalInboxError::DatabaseError {
            source: err,
            message: format!("Failed to look for a reconnected {provider_kind} OAuth grant"),
        })?;

        Ok(reconnected)
    }
}
