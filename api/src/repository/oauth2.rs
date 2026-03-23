use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::{Postgres, QueryBuilder, Transaction};
use uuid::Uuid;

use universal_inbox::{
    auth::oauth2::{
        AuthorizedOAuth2Client, OAuth2AuthorizationCode, OAuth2Client, OAuth2RefreshToken,
    },
    user::UserId,
};

use crate::universal_inbox::UniversalInboxError;

use super::Repository;

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

    async fn revoke_refresh_token(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        token_hash: &str,
    ) -> Result<(), UniversalInboxError>;

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
}

#[derive(sqlx::FromRow)]
struct AuthorizedClientRow {
    client_id: String,
    client_name: Option<String>,
    scope: Option<String>,
    first_authorized_at: NaiveDateTime,
    last_used_at: NaiveDateTime,
}

impl From<AuthorizedClientRow> for AuthorizedOAuth2Client {
    fn from(row: AuthorizedClientRow) -> Self {
        AuthorizedOAuth2Client {
            client_id: row.client_id,
            client_name: row.client_name,
            scope: row.scope,
            first_authorized_at: DateTime::from_naive_utc_and_offset(row.first_authorized_at, Utc),
            last_used_at: DateTime::from_naive_utc_and_offset(row.last_used_at, Utc),
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
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
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
            created_at: DateTime::from_naive_utc_and_offset(row.created_at, Utc),
            updated_at: DateTime::from_naive_utc_and_offset(row.updated_at, Utc),
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
    pub expires_at: NaiveDateTime,
    pub created_at: NaiveDateTime,
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
            expires_at: DateTime::from_naive_utc_and_offset(row.expires_at, Utc),
            created_at: DateTime::from_naive_utc_and_offset(row.created_at, Utc),
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
    pub expires_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
    pub revoked_at: Option<NaiveDateTime>,
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
            expires_at: row
                .expires_at
                .map(|t| DateTime::from_naive_utc_and_offset(t, Utc)),
            created_at: DateTime::from_naive_utc_and_offset(row.created_at, Utc),
            revoked_at: row
                .revoked_at
                .map(|t| DateTime::from_naive_utc_and_offset(t, Utc)),
        }
    }
}
