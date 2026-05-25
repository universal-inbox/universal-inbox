use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Postgres, QueryBuilder, Transaction};
use uuid::Uuid;

use universal_inbox::{
    auth::oauth2::{
        AuthorizedOAuth2Client, OAuth2AuthorizationCode, OAuth2Client, OAuth2RefreshToken,
        OAuth2UserConsent,
    },
    user::UserId,
};

use crate::universal_inbox::{UniversalInboxError, oauth2::cimd::ClientMetadataDocument};

use super::Repository;

/// Cached CIMD metadata document persisted by the AS to amortize fetches.
/// The `client_id` here is the document's URL.
#[derive(Debug, Clone)]
pub struct CachedClientMetadata {
    pub client_id: String,
    pub document: ClientMetadataDocument,
    pub expires_at: DateTime<Utc>,
}

#[allow(clippy::too_many_arguments)]
#[async_trait]
pub trait OAuth2Repository {
    async fn create_oauth2_client(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        client_id: &str,
        client_name: Option<&str>,
        redirect_uris: &[String],
    ) -> Result<OAuth2Client, UniversalInboxError>;

    /// Upsert an `oauth2_client` row for a CIMD-discovered client. Distinct
    /// from [`create_oauth2_client`] because:
    /// (1) the `client_id` is a stable URL chosen by the client itself, not
    ///     a server-minted UUID;
    /// (2) the redirect_uris / client_name may legitimately change between
    ///     CIMD fetches and the row must mirror the current document.
    /// The row exists primarily so existing FK constraints on
    /// `oauth2_authorization_code` / `oauth2_refresh_token` /
    /// `oauth2_user_consent` continue to point at a real row.
    async fn upsert_cimd_oauth2_client(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        client_id_url: &str,
        client_name: Option<&str>,
        redirect_uris: &[String],
    ) -> Result<OAuth2Client, UniversalInboxError>;

    async fn get_oauth2_client_by_client_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        client_id: &str,
    ) -> Result<Option<OAuth2Client>, UniversalInboxError>;

    async fn create_authorization_code(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        code: &str,
        client_id: &str,
        user_id: UserId,
        redirect_uri: &str,
        scope: Option<&str>,
        code_challenge: &str,
        resource: Option<&str>,
        expires_at: DateTime<Utc>,
    ) -> Result<(), UniversalInboxError>;

    async fn get_and_delete_authorization_code(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        code: &str,
    ) -> Result<Option<OAuth2AuthorizationCode>, UniversalInboxError>;

    async fn create_refresh_token(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        token_hash: &str,
        client_id: &str,
        user_id: UserId,
        scope: Option<&str>,
        resource: Option<&str>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<(), UniversalInboxError>;

    async fn get_refresh_token_by_hash(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        token_hash: &str,
    ) -> Result<Option<OAuth2RefreshToken>, UniversalInboxError>;

    /// Look up a refresh token by hash, including revoked ones.
    ///
    /// Used by reuse-detection: when an atomic rotation fails to claim a token
    /// we need to know whether the row exists at all (and, if it does, what
    /// `(client_id, user_id)` family it belongs to so we can revoke the whole
    /// family per RFC 6749 §10.4 / RFC 6819 §5.2.2.3).
    async fn get_refresh_token_by_hash_including_revoked(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        token_hash: &str,
    ) -> Result<Option<OAuth2RefreshToken>, UniversalInboxError>;

    async fn revoke_refresh_token(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        token_hash: &str,
    ) -> Result<(), UniversalInboxError>;

    /// Atomically claim and revoke a refresh token in a single SQL statement.
    ///
    /// Returns `Some(row)` only if exactly one row matched
    /// `token_hash = $1 AND client_id = $2 AND revoked_at IS NULL AND not
    /// expired` — i.e. this caller won the race against any other concurrent
    /// rotation attempt for the same token. Returns `None` if the token was
    /// already revoked, never existed, expired, or belongs to a different
    /// client. The caller MUST treat `None` as a refresh-token-reuse signal
    /// (per RFC 6749 §10.4 / RFC 6819 §5.2.2.3) and revoke the entire token
    /// family for `(client_id, user_id)` when the row exists but was already
    /// consumed.
    async fn revoke_refresh_token_if_active(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        token_hash: &str,
        client_id: &str,
    ) -> Result<Option<OAuth2RefreshToken>, UniversalInboxError>;

    async fn list_authorized_clients(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Vec<AuthorizedOAuth2Client>, UniversalInboxError>;

    async fn revoke_all_refresh_tokens_for_client(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        client_id: &str,
    ) -> Result<u64, UniversalInboxError>;

    async fn get_user_consent(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        client_id: &str,
    ) -> Result<Option<OAuth2UserConsent>, UniversalInboxError>;

    async fn upsert_user_consent(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        client_id: &str,
        scope: &str,
    ) -> Result<OAuth2UserConsent, UniversalInboxError>;

    async fn delete_user_consent(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        client_id: &str,
    ) -> Result<u64, UniversalInboxError>;

    /// Look up a cached CIMD metadata document by URL.
    /// Returns `None` if the row is missing — callers must also check
    /// `expires_at` and refetch when the cache entry has expired.
    async fn get_cimd_metadata_cache(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        client_id_url: &str,
    ) -> Result<Option<CachedClientMetadata>, UniversalInboxError>;

    /// Upsert a freshly-fetched CIMD metadata document.
    async fn upsert_cimd_metadata_cache(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        client_id_url: &str,
        document: &ClientMetadataDocument,
        body_sha256: &[u8],
        expires_at: DateTime<Utc>,
    ) -> Result<(), UniversalInboxError>;
}

#[async_trait]
impl OAuth2Repository for Repository {
    #[tracing::instrument(level = "debug", skip_all, err)]
    async fn create_oauth2_client(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        client_id: &str,
        client_name: Option<&str>,
        redirect_uris: &[String],
    ) -> Result<OAuth2Client, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
                INSERT INTO oauth2_client
                  (client_id, client_name, redirect_uris)
                VALUES (
            "#,
        );
        let mut separated = query_builder.separated(", ");
        separated.push_bind(client_id);
        separated.push_bind(client_name);
        separated.push_bind(redirect_uris);
        query_builder.push(
            r#"
                )
                RETURNING
                  id, client_id, client_name, redirect_uris,
                  grant_types, response_types, token_endpoint_auth_method,
                  created_at, updated_at
            "#,
        );

        let row = query_builder
            .build_query_as::<OAuth2ClientRow>()
            .fetch_one(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to insert new OAuth2 client into storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        Ok(row.into())
    }

    #[tracing::instrument(level = "debug", skip_all, fields(client_id_url), err)]
    async fn upsert_cimd_oauth2_client(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        client_id_url: &str,
        client_name: Option<&str>,
        redirect_uris: &[String],
    ) -> Result<OAuth2Client, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
                INSERT INTO oauth2_client
                  (client_id, client_name, redirect_uris)
                VALUES (
            "#,
        );
        let mut separated = query_builder.separated(", ");
        separated.push_bind(client_id_url);
        separated.push_bind(client_name);
        separated.push_bind(redirect_uris);
        query_builder.push(
            r#"
                )
                ON CONFLICT (client_id) DO UPDATE
                SET client_name    = EXCLUDED.client_name,
                    redirect_uris  = EXCLUDED.redirect_uris,
                    updated_at     = now()
                RETURNING
                  id, client_id, client_name, redirect_uris,
                  grant_types, response_types, token_endpoint_auth_method,
                  created_at, updated_at
            "#,
        );

        let row = query_builder
            .build_query_as::<OAuth2ClientRow>()
            .fetch_one(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to upsert CIMD OAuth2 client into storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        Ok(row.into())
    }

    #[tracing::instrument(level = "debug", skip_all, fields(client_id), err)]
    async fn get_oauth2_client_by_client_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        client_id: &str,
    ) -> Result<Option<OAuth2Client>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
                SELECT
                  id, client_id, client_name, redirect_uris,
                  grant_types, response_types, token_endpoint_auth_method,
                  created_at, updated_at
                FROM oauth2_client
                WHERE client_id =
            "#,
        );
        query_builder.push_bind(client_id);

        let row = query_builder
            .build_query_as::<OAuth2ClientRow>()
            .fetch_optional(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to fetch OAuth2 client from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        Ok(row.map(|r| r.into()))
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(client_id, user.id = user_id.to_string()),
        err
    )]
    async fn create_authorization_code(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        code: &str,
        client_id: &str,
        user_id: UserId,
        redirect_uri: &str,
        scope: Option<&str>,
        code_challenge: &str,
        resource: Option<&str>,
        expires_at: DateTime<Utc>,
    ) -> Result<(), UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
                INSERT INTO oauth2_authorization_code
                  (code, client_id, user_id, redirect_uri, scope,
                   code_challenge, resource, expires_at)
                VALUES (
            "#,
        );
        let mut separated = query_builder.separated(", ");
        separated.push_bind(code);
        separated.push_bind(client_id);
        separated.push_bind(user_id.0);
        separated.push_bind(redirect_uri);
        separated.push_bind(scope);
        separated.push_bind(code_challenge);
        separated.push_bind(resource);
        separated.push_bind(expires_at.naive_utc());
        query_builder.push(")");

        query_builder
            .build()
            .execute(&mut **executor)
            .await
            .map_err(|err| {
                let message =
                    format!("Failed to insert OAuth2 authorization code into storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    async fn get_and_delete_authorization_code(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        code: &str,
    ) -> Result<Option<OAuth2AuthorizationCode>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
                DELETE FROM oauth2_authorization_code
                WHERE code =
            "#,
        );
        query_builder.push_bind(code);
        query_builder.push(
            r#"
                RETURNING
                  code, client_id, user_id, redirect_uri, scope,
                  code_challenge, code_challenge_method, resource,
                  expires_at, created_at
            "#,
        );

        let row = query_builder
            .build_query_as::<OAuth2AuthorizationCodeRow>()
            .fetch_optional(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!(
                    "Failed to fetch and delete OAuth2 authorization code from storage: {err}"
                );
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        Ok(row.map(|r| r.into()))
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(client_id, user.id = user_id.to_string()),
        err
    )]
    async fn create_refresh_token(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        token_hash: &str,
        client_id: &str,
        user_id: UserId,
        scope: Option<&str>,
        resource: Option<&str>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<(), UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
                INSERT INTO oauth2_refresh_token
                  (token_hash, client_id, user_id, scope, resource, expires_at)
                VALUES (
            "#,
        );
        let mut separated = query_builder.separated(", ");
        separated.push_bind(token_hash);
        separated.push_bind(client_id);
        separated.push_bind(user_id.0);
        separated.push_bind(scope);
        separated.push_bind(resource);
        separated.push_bind(expires_at.map(|t| t.naive_utc()));
        query_builder.push(")");

        query_builder
            .build()
            .execute(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to insert OAuth2 refresh token into storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    async fn get_refresh_token_by_hash(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        token_hash: &str,
    ) -> Result<Option<OAuth2RefreshToken>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
                SELECT
                  id, token_hash, client_id, user_id, scope,
                  resource, expires_at, created_at, revoked_at
                FROM oauth2_refresh_token
                WHERE token_hash =
            "#,
        );
        query_builder.push_bind(token_hash);
        query_builder.push(" AND revoked_at IS NULL");

        let row = query_builder
            .build_query_as::<OAuth2RefreshTokenRow>()
            .fetch_optional(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to fetch OAuth2 refresh token from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        Ok(row.map(|r| r.into()))
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    async fn get_refresh_token_by_hash_including_revoked(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        token_hash: &str,
    ) -> Result<Option<OAuth2RefreshToken>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
                SELECT
                  id, token_hash, client_id, user_id, scope,
                  resource, expires_at, created_at, revoked_at
                FROM oauth2_refresh_token
                WHERE token_hash =
            "#,
        );
        query_builder.push_bind(token_hash);

        let row = query_builder
            .build_query_as::<OAuth2RefreshTokenRow>()
            .fetch_optional(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!(
                    "Failed to fetch OAuth2 refresh token (including revoked) from storage: {err}"
                );
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        Ok(row.map(|r| r.into()))
    }

    #[tracing::instrument(level = "debug", skip_all, fields(client_id), err)]
    async fn revoke_refresh_token_if_active(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        token_hash: &str,
        client_id: &str,
    ) -> Result<Option<OAuth2RefreshToken>, UniversalInboxError> {
        // Single atomic statement: only the caller whose UPDATE actually flips
        // `revoked_at` from NULL gets the row back. Concurrent rotations for
        // the same token will see RETURNING produce zero rows (Postgres' row
        // lock on the matching tuple serializes the writers — losers re-read
        // a row that no longer satisfies `revoked_at IS NULL`).
        //
        // The expiry check is folded into the WHERE clause so we never
        // "claim" (and therefore never mint new tokens from) an expired row;
        // an expired row is indistinguishable from an already-revoked or
        // never-existed row at this layer — the service treats all three as
        // `invalid_grant`.
        let mut query_builder = QueryBuilder::new(
            r#"
                UPDATE oauth2_refresh_token
                SET revoked_at = now()
                WHERE token_hash =
            "#,
        );
        query_builder.push_bind(token_hash);
        query_builder.push(" AND client_id = ");
        query_builder.push_bind(client_id);
        query_builder.push(
            r#"
                AND revoked_at IS NULL
                AND (expires_at IS NULL OR expires_at > now())
                RETURNING
                  id, token_hash, client_id, user_id, scope,
                  resource, expires_at, created_at, revoked_at
            "#,
        );

        let row = query_builder
            .build_query_as::<OAuth2RefreshTokenRow>()
            .fetch_optional(&mut **executor)
            .await
            .map_err(|err| {
                let message =
                    format!("Failed to atomically revoke OAuth2 refresh token in storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        Ok(row.map(|r| r.into()))
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    async fn revoke_refresh_token(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        token_hash: &str,
    ) -> Result<(), UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
                UPDATE oauth2_refresh_token
                SET revoked_at = now()
                WHERE token_hash =
            "#,
        );
        query_builder.push_bind(token_hash);
        query_builder.push(" AND revoked_at IS NULL");

        query_builder
            .build()
            .execute(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to revoke OAuth2 refresh token in storage: {err}");
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
        fields(user.id = user_id.to_string()),
        err
    )]
    async fn list_authorized_clients(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Vec<AuthorizedOAuth2Client>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
                SELECT
                  rt.client_id,
                  c.client_name,
                  rt.scope,
                  MIN(rt.created_at) AS first_authorized_at,
                  MAX(rt.created_at) AS last_used_at
                FROM oauth2_refresh_token rt
                JOIN oauth2_client c ON c.client_id = rt.client_id
                WHERE rt.user_id =
            "#,
        );
        query_builder.push_bind(user_id.0);
        query_builder.push(
            r#"
                AND rt.revoked_at IS NULL
                AND (rt.expires_at IS NULL OR rt.expires_at > now())
                GROUP BY rt.client_id, c.client_name, rt.scope
            "#,
        );

        let rows = query_builder
            .build_query_as::<AuthorizedClientRow>()
            .fetch_all(&mut **executor)
            .await
            .map_err(|err| {
                let message =
                    format!("Failed to fetch authorized OAuth2 clients from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(client_id, user.id = user_id.to_string()),
        err
    )]
    async fn revoke_all_refresh_tokens_for_client(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        client_id: &str,
    ) -> Result<u64, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
                UPDATE oauth2_refresh_token
                SET revoked_at = now()
                WHERE user_id =
            "#,
        );
        query_builder.push_bind(user_id.0);
        query_builder.push(" AND client_id = ");
        query_builder.push_bind(client_id);
        query_builder.push(" AND revoked_at IS NULL");

        let result = query_builder
            .build()
            .execute(&mut **executor)
            .await
            .map_err(|err| {
                let message =
                    format!("Failed to revoke OAuth2 refresh tokens for client in storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        Ok(result.rows_affected())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(client_id, user.id = user_id.to_string()),
        err
    )]
    async fn get_user_consent(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        client_id: &str,
    ) -> Result<Option<OAuth2UserConsent>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
                SELECT user_id, client_id, scope, granted_at
                FROM oauth2_user_consent
                WHERE user_id =
            "#,
        );
        query_builder.push_bind(user_id.0);
        query_builder.push(" AND client_id = ");
        query_builder.push_bind(client_id);

        let row = query_builder
            .build_query_as::<OAuth2UserConsentRow>()
            .fetch_optional(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to fetch OAuth2 user consent from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        Ok(row.map(|r| r.into()))
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(client_id, user.id = user_id.to_string()),
        err
    )]
    async fn upsert_user_consent(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        client_id: &str,
        scope: &str,
    ) -> Result<OAuth2UserConsent, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
                INSERT INTO oauth2_user_consent (user_id, client_id, scope)
                VALUES (
            "#,
        );
        let mut separated = query_builder.separated(", ");
        separated.push_bind(user_id.0);
        separated.push_bind(client_id);
        separated.push_bind(scope);
        query_builder.push(
            r#"
                )
                ON CONFLICT (user_id, client_id) DO UPDATE
                SET scope = EXCLUDED.scope, granted_at = now()
                RETURNING user_id, client_id, scope, granted_at
            "#,
        );

        let row = query_builder
            .build_query_as::<OAuth2UserConsentRow>()
            .fetch_one(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to upsert OAuth2 user consent into storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        Ok(row.into())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(client_id, user.id = user_id.to_string()),
        err
    )]
    async fn delete_user_consent(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        client_id: &str,
    ) -> Result<u64, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
                DELETE FROM oauth2_user_consent
                WHERE user_id =
            "#,
        );
        query_builder.push_bind(user_id.0);
        query_builder.push(" AND client_id = ");
        query_builder.push_bind(client_id);

        let result = query_builder
            .build()
            .execute(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to delete OAuth2 user consent from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        Ok(result.rows_affected())
    }

    #[tracing::instrument(level = "debug", skip_all, fields(client_id_url), err)]
    async fn get_cimd_metadata_cache(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        client_id_url: &str,
    ) -> Result<Option<CachedClientMetadata>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
                SELECT client_id, document, expires_at
                FROM oauth2_client_metadata_cache
                WHERE client_id =
            "#,
        );
        query_builder.push_bind(client_id_url);

        let row = query_builder
            .build_query_as::<OAuth2ClientMetadataCacheRow>()
            .fetch_optional(&mut **executor)
            .await
            .map_err(|err| {
                let message =
                    format!("Failed to fetch CIMD metadata cache row from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        row.map(CachedClientMetadata::try_from).transpose()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(client_id_url), err)]
    async fn upsert_cimd_metadata_cache(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        client_id_url: &str,
        document: &ClientMetadataDocument,
        body_sha256: &[u8],
        expires_at: DateTime<Utc>,
    ) -> Result<(), UniversalInboxError> {
        let document_value = serde_json::to_value(document).map_err(|err| {
            UniversalInboxError::Unexpected(anyhow::anyhow!(
                "Failed to serialize CIMD document for storage: {err}"
            ))
        })?;

        let mut query_builder = QueryBuilder::new(
            r#"
                INSERT INTO oauth2_client_metadata_cache
                  (client_id, document, body_sha256, expires_at)
                VALUES (
            "#,
        );
        let mut separated = query_builder.separated(", ");
        separated.push_bind(client_id_url);
        separated.push_bind(document_value);
        separated.push_bind(body_sha256);
        separated.push_bind(expires_at);
        query_builder.push(
            r#"
                )
                ON CONFLICT (client_id) DO UPDATE
                SET document    = EXCLUDED.document,
                    body_sha256 = EXCLUDED.body_sha256,
                    fetched_at  = now(),
                    expires_at  = EXCLUDED.expires_at
            "#,
        );

        query_builder
            .build()
            .execute(&mut **executor)
            .await
            .map_err(|err| {
                let message =
                    format!("Failed to upsert CIMD metadata cache row into storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct OAuth2ClientMetadataCacheRow {
    client_id: String,
    document: serde_json::Value,
    expires_at: DateTime<Utc>,
}

impl TryFrom<OAuth2ClientMetadataCacheRow> for CachedClientMetadata {
    type Error = UniversalInboxError;

    fn try_from(row: OAuth2ClientMetadataCacheRow) -> Result<Self, Self::Error> {
        let document: ClientMetadataDocument =
            serde_json::from_value(row.document).map_err(|err| {
                UniversalInboxError::Unexpected(anyhow::anyhow!(
                    "Stored CIMD document for {} is no longer parseable as ClientMetadataDocument: {err}",
                    row.client_id
                ))
            })?;
        Ok(CachedClientMetadata {
            client_id: row.client_id,
            document,
            expires_at: row.expires_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct OAuth2UserConsentRow {
    user_id: Uuid,
    client_id: String,
    scope: String,
    granted_at: DateTime<Utc>,
}

impl From<OAuth2UserConsentRow> for OAuth2UserConsent {
    fn from(row: OAuth2UserConsentRow) -> Self {
        OAuth2UserConsent {
            user_id: row.user_id.into(),
            client_id: row.client_id,
            scope: row.scope,
            granted_at: row.granted_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct AuthorizedClientRow {
    client_id: String,
    client_name: Option<String>,
    scope: Option<String>,
    first_authorized_at: DateTime<Utc>,
    last_used_at: DateTime<Utc>,
}

impl From<AuthorizedClientRow> for AuthorizedOAuth2Client {
    fn from(row: AuthorizedClientRow) -> Self {
        AuthorizedOAuth2Client {
            client_id: row.client_id,
            client_name: row.client_name,
            scope: row.scope,
            first_authorized_at: row.first_authorized_at,
            last_used_at: row.last_used_at,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct OAuth2ClientRow {
    pub id: Uuid,
    pub client_id: String,
    pub client_name: Option<String>,
    pub redirect_uris: Vec<String>,
    pub grant_types: Vec<String>,
    pub response_types: Vec<String>,
    pub token_endpoint_auth_method: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<OAuth2ClientRow> for OAuth2Client {
    fn from(row: OAuth2ClientRow) -> Self {
        OAuth2Client {
            id: row.id,
            client_id: row.client_id,
            client_name: row.client_name,
            redirect_uris: row.redirect_uris,
            grant_types: row.grant_types,
            response_types: row.response_types,
            token_endpoint_auth_method: row.token_endpoint_auth_method,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct OAuth2AuthorizationCodeRow {
    pub code: String,
    pub client_id: String,
    pub user_id: Uuid,
    pub redirect_uri: String,
    pub scope: Option<String>,
    pub code_challenge: String,
    pub code_challenge_method: String,
    pub resource: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl From<OAuth2AuthorizationCodeRow> for OAuth2AuthorizationCode {
    fn from(row: OAuth2AuthorizationCodeRow) -> Self {
        OAuth2AuthorizationCode {
            code: row.code,
            client_id: row.client_id,
            user_id: row.user_id.into(),
            redirect_uri: row.redirect_uri,
            scope: row.scope,
            code_challenge: row.code_challenge,
            code_challenge_method: row.code_challenge_method,
            resource: row.resource,
            expires_at: row.expires_at,
            created_at: row.created_at,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct OAuth2RefreshTokenRow {
    pub id: Uuid,
    pub token_hash: String,
    pub client_id: String,
    pub user_id: Uuid,
    pub scope: Option<String>,
    pub resource: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl From<OAuth2RefreshTokenRow> for OAuth2RefreshToken {
    fn from(row: OAuth2RefreshTokenRow) -> Self {
        OAuth2RefreshToken {
            id: row.id,
            token_hash: row.token_hash,
            client_id: row.client_id,
            user_id: row.user_id.into(),
            scope: row.scope,
            resource: row.resource,
            expires_at: row.expires_at,
            created_at: row.created_at,
            revoked_at: row.revoked_at,
        }
    }
}
