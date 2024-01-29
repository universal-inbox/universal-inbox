use anyhow::Context;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use secrecy::{ExposeSecret, Secret};
use sqlx::{Postgres, Transaction};

use universal_inbox::{
    auth::auth_token::{AuthenticationToken, JWTToken},
    user::UserId,
};
use uuid::Uuid;

use crate::universal_inbox::UniversalInboxError;

use super::Repository;

#[async_trait]
pub trait AuthenticationTokenRepository {
    async fn create_auth_token<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        auth_token: AuthenticationToken,
    ) -> Result<AuthenticationToken, UniversalInboxError>;

    async fn fetch_auth_tokens_for_user<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        user_id: UserId,
    ) -> Result<Vec<AuthenticationToken>, UniversalInboxError>;
}

#[async_trait]
impl AuthenticationTokenRepository for Repository {
    #[tracing::instrument(level = "debug", skip(self, executor, auth_token))]
    async fn create_auth_token<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
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
                    jwt_token
                  )
                VALUES
                  (
                    $1,
                    $2,
                    $3,
                    $4,
                    $5
                  )
            "#,
            auth_token.id.0,
            auth_token.created_at.naive_utc(),
            auth_token.updated_at.naive_utc(),
            auth_token.user_id.0,
            auth_token.jwt_token.expose_secret().0,
        )
        .execute(&mut **executor)
        .await
        .context("Failed to insert new authentication token into storage")?;

        Ok(auth_token)
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    async fn fetch_auth_tokens_for_user<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        user_id: UserId,
    ) -> Result<Vec<AuthenticationToken>, UniversalInboxError> {
        let rows = sqlx::query_as!(
            AuthenticationTokenRow,
            r#"
                SELECT
                  id,
                  created_at,
                  updated_at,
                  user_id,
                  jwt_token,
                  expire_at,
                  is_revoked
                FROM
                  authentication_token
                WHERE
                  user_id = $1
            "#,
            user_id.0,
        )
        .fetch_all(&mut **executor)
        .await
        .context("Failed to fetch authentication tokens from storage for user {user_id}")?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct AuthenticationTokenRow {
    pub id: Uuid,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub user_id: Uuid,
    pub jwt_token: String,
    pub expire_at: Option<NaiveDateTime>,
    pub is_revoked: bool,
}

impl From<AuthenticationTokenRow> for AuthenticationToken {
    fn from(row: AuthenticationTokenRow) -> Self {
        (&row).into()
    }
}

impl From<&AuthenticationTokenRow> for AuthenticationToken {
    fn from(row: &AuthenticationTokenRow) -> Self {
        AuthenticationToken {
            id: row.id.into(),
            created_at: DateTime::from_naive_utc_and_offset(row.created_at, Utc),
            updated_at: DateTime::from_naive_utc_and_offset(row.updated_at, Utc),
            user_id: row.user_id.into(),
            jwt_token: Secret::new(JWTToken(row.jwt_token.clone())),
            expire_at: row
                .expire_at
                .map(|expire_at| DateTime::from_naive_utc_and_offset(expire_at, Utc)),
            is_revoked: row.is_revoked,
        }
    }
}
