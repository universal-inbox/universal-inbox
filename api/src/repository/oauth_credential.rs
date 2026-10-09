use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use universal_inbox::{
    integration_connection::IntegrationConnectionId,
    integration_connection::provider::IntegrationProviderKind, user::UserId,
};

use crate::{
    observability::attr,
    repository::Repository,
    universal_inbox::UniversalInboxError,
    utils::crypto::{aad, data_keyring},
};

const RAW_TOKEN_RESPONSE_AAD: &str = "oauth_credential.raw_token_response";

fn encrypt_raw_token_response(
    raw_token_response: &serde_json::Value,
    integration_connection_id: IntegrationConnectionId,
) -> Result<Vec<u8>, UniversalInboxError> {
    data_keyring()?.encrypt_json(
        raw_token_response,
        &aad(RAW_TOKEN_RESPONSE_AAD, integration_connection_id.into()),
    )
}

/// Decrypt `oauth_credential.raw_token_response_enc`. A credential stored before encryption
/// and not encrypted yet by `data-encryption encrypt-plaintext` reads as an empty response.
fn decrypt_raw_token_response(
    raw_token_response_enc: Option<&[u8]>,
    integration_connection_id: Uuid,
) -> Result<serde_json::Value, UniversalInboxError> {
    match raw_token_response_enc {
        Some(raw_token_response_enc) => data_keyring()?.decrypt_json(
            raw_token_response_enc,
            &aad(RAW_TOKEN_RESPONSE_AAD, integration_connection_id),
        ),
        None => Ok(serde_json::json!({})),
    }
}

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
    pub user_id: UserId,
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

    /// Same as [`Self::get_oauth_credential`], but locks the row until the
    /// transaction ends, so the refresh cron (`FOR UPDATE SKIP LOCKED`) cannot
    /// rotate the tokens while they are being revoked.
    async fn lock_oauth_credential(
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
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::INTEGRATION_CONNECTION_ID } = integration_connection_id.to_string())
    )]
    async fn store_oauth_credential(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        integration_connection_id: IntegrationConnectionId,
        encrypted_access_token: Vec<u8>,
        encrypted_refresh_token: Option<Vec<u8>>,
        access_token_expires_at: Option<DateTime<Utc>>,
        mut raw_token_response: serde_json::Value,
    ) -> Result<StoredOAuthCredential, UniversalInboxError> {
        // Last line of defence: whatever the caller passed, never persist a
        // cleartext credential next to its encrypted copy.
        strip_credential_fields(&mut raw_token_response);
        let now = Utc::now();
        let row = sqlx::query!(
            r#"
                INSERT INTO oauth_credential
                  (integration_connection_id, encrypted_access_token, encrypted_refresh_token,
                   access_token_expires_at, raw_token_response_enc, created_at, updated_at)
                VALUES
                  ($1, $2, $3, $4, $5, $6, $7)
                ON CONFLICT (integration_connection_id) DO UPDATE SET
                  encrypted_access_token = EXCLUDED.encrypted_access_token,
                  encrypted_refresh_token = COALESCE(EXCLUDED.encrypted_refresh_token, oauth_credential.encrypted_refresh_token),
                  access_token_expires_at = EXCLUDED.access_token_expires_at,
                  raw_token_response_enc = EXCLUDED.raw_token_response_enc,
                  raw_token_response = NULL,
                  updated_at = EXCLUDED.updated_at
                RETURNING
                  integration_connection_id,
                  encrypted_access_token,
                  encrypted_refresh_token,
                  access_token_expires_at,
                  raw_token_response_enc,
                  created_at,
                  updated_at
            "#,
            Uuid::from(integration_connection_id),
            &encrypted_access_token,
            encrypted_refresh_token.as_deref(),
            access_token_expires_at,
            encrypt_raw_token_response(&raw_token_response, integration_connection_id)?,
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
            raw_token_response: decrypt_raw_token_response(
                row.raw_token_response_enc.as_deref(),
                row.integration_connection_id,
            )?,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

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
                  raw_token_response_enc,
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

        row.map(|row| {
            Ok(StoredOAuthCredential {
                integration_connection_id: IntegrationConnectionId(row.integration_connection_id),
                raw_token_response: decrypt_raw_token_response(
                    row.raw_token_response_enc.as_deref(),
                    row.integration_connection_id,
                )?,
                encrypted_access_token: row.encrypted_access_token,
                encrypted_refresh_token: row.encrypted_refresh_token,
                access_token_expires_at: row.access_token_expires_at,
                created_at: row.created_at,
                updated_at: row.updated_at,
            })
        })
        .transpose()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::INTEGRATION_CONNECTION_ID } = integration_connection_id.to_string())
    )]
    async fn lock_oauth_credential(
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
                  raw_token_response_enc,
                  created_at,
                  updated_at
                FROM oauth_credential
                WHERE integration_connection_id = $1
                FOR UPDATE
            "#,
            Uuid::from(integration_connection_id),
        )
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!(
                "Failed to lock OAuth credential for integration connection {integration_connection_id}: {err}"
            );
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        row.map(|row| {
            Ok(StoredOAuthCredential {
                integration_connection_id: IntegrationConnectionId(row.integration_connection_id),
                raw_token_response: decrypt_raw_token_response(
                    row.raw_token_response_enc.as_deref(),
                    row.integration_connection_id,
                )?,
                encrypted_access_token: row.encrypted_access_token,
                encrypted_refresh_token: row.encrypted_refresh_token,
                access_token_expires_at: row.access_token_expires_at,
                created_at: row.created_at,
                updated_at: row.updated_at,
            })
        })
        .transpose()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::INTEGRATION_CONNECTION_ID } = integration_connection_id.to_string())
    )]
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

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::INTEGRATION_PROVIDER_KIND } = provider_kind.map(|kind| kind.to_string()))
    )]
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
                  ic.user_id,
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
                    user_id: UserId(row.user_id),
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

/// Keys whose values are provider credentials. They are stored encrypted in
/// their own columns and must never appear in `raw_token_response`.
const CREDENTIAL_FIELDS: [&str; 3] = ["access_token", "refresh_token", "id_token"];

/// Remove every [`CREDENTIAL_FIELDS`] key from `value`, at any depth.
pub fn strip_credential_fields(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.retain(|key, _| !CREDENTIAL_FIELDS.contains(&key.as_str()));
            map.values_mut().for_each(strip_credential_fields);
        }
        serde_json::Value::Array(values) => values.iter_mut().for_each(strip_credential_fields),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_strip_credential_fields_at_any_depth() {
        let mut raw = json!({
            "ok": true,
            "access_token": "xoxe.xoxp-secret",
            "refresh_token": "xoxe-1-secret",
            "scope": "channels:read",
            "authed_user": { "id": "U1", "access_token": "xoxp-user-secret" },
            "extra": [{ "id_token": "eyJ-secret", "kept": 1 }]
        });

        strip_credential_fields(&mut raw);

        assert_eq!(
            raw,
            json!({
                "ok": true,
                "scope": "channels:read",
                "authed_user": { "id": "U1" },
                "extra": [{ "kept": 1 }]
            })
        );
    }
}
