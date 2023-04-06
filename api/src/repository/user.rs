use anyhow::Context;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use universal_inbox::user::{AuthUserId, User, UserId};

use crate::universal_inbox::UniversalInboxError;

use super::Repository;

#[async_trait]
pub trait UserRepository {
    async fn get_user<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        id: UserId,
    ) -> Result<Option<User>, UniversalInboxError>;

    async fn fetch_all_users<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
    ) -> Result<Vec<User>, UniversalInboxError>;

    async fn get_user_by_auth_id<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        auth_user_id: AuthUserId,
    ) -> Result<Option<User>, UniversalInboxError>;

    async fn create_user<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        user: User,
    ) -> Result<User, UniversalInboxError>;
}

#[async_trait]
impl UserRepository for Repository {
    #[tracing::instrument(level = "debug", skip(self))]
    async fn get_user<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        id: UserId,
    ) -> Result<Option<User>, UniversalInboxError> {
        let row = sqlx::query_as!(
            UserRow,
            r#"
                SELECT
                  id,
                  auth_user_id,
                  first_name,
                  last_name,
                  email,
                  created_at,
                  updated_at
                FROM "user"
                WHERE id = $1
            "#,
            id.0
        )
        .fetch_optional(executor)
        .await
        .with_context(|| format!("Failed to fetch user {id} from storage"))?;

        Ok(row.map(|user_row| user_row.into()))
    }

    async fn fetch_all_users<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
    ) -> Result<Vec<User>, UniversalInboxError> {
        let rows = sqlx::query_as!(
            UserRow,
            r#"
                SELECT
                  id,
                  auth_user_id,
                  first_name,
                  last_name,
                  email,
                  created_at,
                  updated_at
                FROM "user"
            "#
        )
        .fetch_all(executor)
        .await
        .context("Failed to fetch all users from storage")?;

        Ok(rows.iter().map(|r| r.into()).collect())
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn get_user_by_auth_id<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        auth_user_id: AuthUserId,
    ) -> Result<Option<User>, UniversalInboxError> {
        let row = sqlx::query_as!(
            UserRow,
            r#"
                SELECT
                  id,
                  auth_user_id,
                  first_name,
                  last_name,
                  email,
                  created_at,
                  updated_at
                FROM "user"
                WHERE auth_user_id = $1
            "#,
            auth_user_id.0
        )
        .fetch_optional(executor)
        .await
        .with_context(|| {
            format!("Failed to fetch user with auth provider user ID {auth_user_id} from storage")
        })?;

        Ok(row.map(|user_row| user_row.into()))
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn create_user<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        user: User,
    ) -> Result<User, UniversalInboxError> {
        let id = UserId(
            sqlx::query_scalar!(
                r#"
                INSERT INTO "user"
                  (
                    id,
                    auth_user_id,
                    first_name,
                    last_name,
                    email,
                    created_at,
                    updated_at
                  )
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                RETURNING
                  id
            "#,
                user.id.0,
                user.auth_user_id.0,
                user.first_name,
                user.last_name,
                user.email,
                user.created_at.naive_utc(),
                user.updated_at.naive_utc()
            )
            .fetch_one(executor)
            .await
            .with_context(|| {
                format!(
                    "Failed to create new user from auth provider user ID {}",
                    user.auth_user_id
                )
            })?,
        );

        Ok(User { id, ..user })
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct UserRow {
    id: Uuid,
    auth_user_id: String,
    first_name: String,
    last_name: String,
    email: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
}

impl From<UserRow> for User {
    fn from(row: UserRow) -> Self {
        (&row).into()
    }
}

impl From<&UserRow> for User {
    fn from(row: &UserRow) -> Self {
        User {
            id: row.id.into(),
            auth_user_id: row.auth_user_id.clone().into(),
            first_name: row.first_name.clone(),
            last_name: row.last_name.clone(),
            email: row.email.clone(),
            created_at: DateTime::<Utc>::from_utc(row.created_at, Utc),
            updated_at: DateTime::<Utc>::from_utc(row.updated_at, Utc),
        }
    }
}
