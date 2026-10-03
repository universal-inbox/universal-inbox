use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use secrecy::ExposeSecret;
use sqlx::{Postgres, QueryBuilder, Transaction};

use universal_inbox::{
    auth::auth_token::{AuthenticationToken, AuthenticationTokenId, TruncatedAuthenticationToken},
    user::UserId,
};
use uuid::Uuid;

use crate::observability::attr;
use crate::universal_inbox::UniversalInboxError;

use super::Repository;

#[async_trait]
pub trait AuthenticationTokenRepository {
    async fn create_auth_token(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        auth_token: AuthenticationToken,
    ) -> Result<AuthenticationToken, UniversalInboxError>;

    async fn fetch_auth_tokens_for_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        exclude_session_tokens: bool,
    ) -> Result<Vec<TruncatedAuthenticationToken>, UniversalInboxError>;

    /// Revocation state of the stored token whose SHA-256 digest is
    /// `jwt_token_hash`: `(is_revoked, expire_at)`, or `None` when no such
    /// token is stored.
    async fn get_auth_token_status_by_hash(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        jwt_token_hash: &str,
    ) -> Result<Option<(bool, Option<DateTime<Utc>>)>, UniversalInboxError>;

    /// Revoke one of the user's stored tokens. Returns `false` when the user
    /// has no such token.
    async fn revoke_auth_token(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        auth_token_id: AuthenticationTokenId,
    ) -> Result<bool, UniversalInboxError>;
}

#[async_trait]
impl AuthenticationTokenRepository for Repository {
    #[tracing::instrument(level = "debug", skip_all, err)]
    async fn create_auth_token(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        auth_token: AuthenticationToken,
    ) -> Result<AuthenticationToken, UniversalInboxError> {
        sqlx::query!(
            r#"
                INSERT INTO authentication_token
                  (
                    id,
                    created_at,
                    updated_at,
                    user_id,
                    jwt_token_hash,
                    truncated_jwt_token,
                    expire_at,
                    is_session_token
                  )
                VALUES
                  (
                    $1,
                    $2,
                    $3,
                    $4,
                    $5,
                    $6,
                    $7,
                    $8
                  )
            "#,
            auth_token.id.0,
            auth_token.created_at.naive_utc(),
            auth_token.updated_at.naive_utc(),
            auth_token.user_id.0,
            hash_jwt_token(&auth_token.jwt_token.expose_secret().0),
            TruncatedAuthenticationToken::truncate(auth_token.jwt_token.expose_secret()),
            auth_token.expire_at.map(|expire_at| expire_at.naive_utc()),
            auth_token.is_session_token,
        )
        .execute(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!("Failed to insert new authentication token into storage: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        Ok(auth_token)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string(), { attr::AUTH_TOKEN_EXCLUDE_SESSION } = exclude_session_tokens),
        err
    )]
    async fn fetch_auth_tokens_for_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        exclude_session_tokens: bool,
    ) -> Result<Vec<TruncatedAuthenticationToken>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
                SELECT
                  id,
                  user_id,
                  truncated_jwt_token,
                  expire_at,
                  is_revoked,
                  is_session_token
                FROM
                  authentication_token
                WHERE
                  user_id =
            "#,
        );
        query_builder.push_bind(user_id.0);
        if exclude_session_tokens {
            query_builder.push(" AND is_session_token = false");
        }
        query_builder.push(" ORDER BY created_at DESC");

        let rows = query_builder
            .build_query_as::<AuthenticationTokenRow>()
            .fetch_all(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to fetch authentication tokens from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    async fn get_auth_token_status_by_hash(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        jwt_token_hash: &str,
    ) -> Result<Option<(bool, Option<DateTime<Utc>>)>, UniversalInboxError> {
        let row = sqlx::query!(
            r#"
                SELECT is_revoked, expire_at
                FROM authentication_token
                WHERE jwt_token_hash = $1
            "#,
            jwt_token_hash
        )
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!("Failed to fetch authentication token status: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        Ok(row.map(|row| {
            (
                row.is_revoked,
                row.expire_at
                    .map(|expire_at| DateTime::from_naive_utc_and_offset(expire_at, Utc)),
            )
        }))
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string(), { attr::AUTH_TOKEN_ID } = auth_token_id.to_string()),
        err
    )]
    async fn revoke_auth_token(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        auth_token_id: AuthenticationTokenId,
    ) -> Result<bool, UniversalInboxError> {
        let result = sqlx::query!(
            r#"
                UPDATE authentication_token
                SET is_revoked = true, updated_at = $3
                WHERE id = $1 AND user_id = $2
            "#,
            auth_token_id.0,
            user_id.0,
            Utc::now().naive_utc()
        )
        .execute(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!("Failed to revoke authentication token {auth_token_id}: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        Ok(result.rows_affected() > 0)
    }
}

/// SHA-256 digest (hex) of a bearer token, as stored in
/// `authentication_token.jwt_token_hash`. The tokens are signed JWTs with far
/// more entropy than a password, so an unkeyed digest is enough (the same
/// choice as `oauth2_refresh_token.token_hash`).
pub fn hash_jwt_token(jwt_token: &str) -> String {
    hex::encode(ring::digest::digest(
        &ring::digest::SHA256,
        jwt_token.as_bytes(),
    ))
}

#[derive(Debug, sqlx::FromRow)]
pub struct AuthenticationTokenRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub truncated_jwt_token: String,
    pub expire_at: Option<NaiveDateTime>,
    pub is_revoked: bool,
    pub is_session_token: bool,
}

impl From<AuthenticationTokenRow> for TruncatedAuthenticationToken {
    fn from(row: AuthenticationTokenRow) -> Self {
        TruncatedAuthenticationToken {
            id: row.id.into(),
            user_id: row.user_id.into(),
            truncated_jwt_token: row.truncated_jwt_token,
            expire_at: row
                .expire_at
                .map(|expire_at| DateTime::from_naive_utc_and_offset(expire_at, Utc)),
            is_revoked: row.is_revoked,
            is_session_token: row.is_session_token,
        }
    }
}
