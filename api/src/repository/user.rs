use anyhow::{anyhow, Context};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use email_address::EmailAddress;
use secrecy::{ExposeSecret, Secret};
use sqlx::{Postgres, QueryBuilder, Transaction};
use uuid::Uuid;

use universal_inbox::{
    auth::AuthIdToken,
    user::{
        AuthUserId, LocalUserAuth, OpenIdConnectUserAuth, PasswordHash, User, UserAuth, UserId,
    },
};

use crate::universal_inbox::{UniversalInboxError, UpdateStatus};

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

    async fn get_user_by_email<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        email: &EmailAddress,
    ) -> Result<Option<User>, UniversalInboxError>;

    async fn create_user<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        user: User,
    ) -> Result<User, UniversalInboxError>;
    async fn update_user_auth_id_token<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        auth_user_id: &AuthUserId,
        auth_id_token: &AuthIdToken,
    ) -> Result<UpdateStatus<User>, UniversalInboxError>;
}

#[async_trait]
impl UserRepository for Repository {
    #[tracing::instrument(level = "debug", skip(self, executor))]
    async fn get_user<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        id: UserId,
    ) -> Result<Option<User>, UniversalInboxError> {
        let row = sqlx::query_as!(
            UserRow,
            r#"
                SELECT
                  "user".id,
                  "user".first_name,
                  "user".last_name,
                  "user".email,
                  "user".created_at,
                  "user".updated_at,
                  user_auth.kind as "user_auth_kind: _",
                  user_auth.auth_user_id,
                  user_auth.auth_id_token,
                  user_auth.password_hash
                FROM "user"
                INNER JOIN user_auth ON user_auth.user_id = "user".id
                WHERE "user".id = $1
            "#,
            id.0
        )
        .fetch_optional(&mut **executor)
        .await
        .with_context(|| format!("Failed to fetch user {id} from storage"))?;

        row.map(|user_row| user_row.try_into()).transpose()
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    async fn fetch_all_users<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
    ) -> Result<Vec<User>, UniversalInboxError> {
        let rows = sqlx::query_as!(
            UserRow,
            r#"
                SELECT
                  "user".id,
                  "user".first_name,
                  "user".last_name,
                  "user".email,
                  "user".created_at,
                  "user".updated_at,
                  user_auth.kind as "user_auth_kind: _",
                  user_auth.auth_user_id,
                  user_auth.auth_id_token,
                  user_auth.password_hash
                FROM "user"
                INNER JOIN user_auth ON user_auth.user_id = "user".id
            "#
        )
        .fetch_all(&mut **executor)
        .await
        .context("Failed to fetch all users from storage")?;

        rows.iter().map(|r| r.try_into()).collect()
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    async fn get_user_by_auth_id<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        auth_user_id: AuthUserId,
    ) -> Result<Option<User>, UniversalInboxError> {
        let row = sqlx::query_as!(
            UserRow,
            r#"
                SELECT
                  "user".id,
                  "user".first_name,
                  "user".last_name,
                  "user".email,
                  "user".created_at,
                  "user".updated_at,
                  user_auth.kind as "user_auth_kind: _",
                  user_auth.auth_user_id,
                  user_auth.auth_id_token,
                  user_auth.password_hash
                FROM "user"
                INNER JOIN user_auth ON user_auth.user_id = "user".id
                WHERE user_auth.auth_user_id = $1
            "#,
            auth_user_id.0
        )
        .fetch_optional(&mut **executor)
        .await
        .with_context(|| {
            format!("Failed to fetch user with auth provider user ID {auth_user_id} from storage")
        })?;

        row.map(|user_row| user_row.try_into()).transpose()
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    async fn get_user_by_email<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        email: &EmailAddress,
    ) -> Result<Option<User>, UniversalInboxError> {
        let row = sqlx::query_as!(
            UserRow,
            r#"
                SELECT
                  "user".id,
                  "user".first_name,
                  "user".last_name,
                  "user".email,
                  "user".created_at,
                  "user".updated_at,
                  user_auth.kind as "user_auth_kind: _",
                  user_auth.auth_user_id,
                  user_auth.auth_id_token,
                  user_auth.password_hash
                FROM "user"
                INNER JOIN user_auth ON user_auth.user_id = "user".id
                WHERE "user".email = $1
            "#,
            email.to_string()
        )
        .fetch_optional(&mut **executor)
        .await
        .context("Failed to fetch user by email from storage")?;

        row.map(|user_row| user_row.try_into()).transpose()
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    async fn create_user<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        user: User,
    ) -> Result<User, UniversalInboxError> {
        let user_id = UserId(
            sqlx::query_scalar!(
                r#"
                INSERT INTO "user"
                  (
                    id,
                    first_name,
                    last_name,
                    email,
                    created_at,
                    updated_at
                  )
                VALUES ($1, $2, $3, $4, $5, $6)
                RETURNING
                  id
            "#,
                user.id.0,
                user.first_name,
                user.last_name,
                user.email.to_string(),
                user.created_at.naive_utc(),
                user.updated_at.naive_utc()
            )
            .fetch_one(&mut **executor)
            .await
            .map_err(|e| {
                match e
                    .as_database_error()
                    .and_then(|db_error| db_error.code().map(|code| code.to_string()))
                {
                    Some(x) if x == *"23505" => UniversalInboxError::AlreadyExists {
                        source: e,
                        id: user.id.0,
                    },
                    _ => UniversalInboxError::Unexpected(anyhow!("Failed to create new user")),
                }
            })?,
        );

        let (auth_user_id, auth_id_token, password_hash) = match user.auth {
            UserAuth::OpenIdConnect(OpenIdConnectUserAuth {
                ref auth_user_id,
                ref auth_id_token,
            }) => (Some(auth_user_id), Some(auth_id_token), None),
            UserAuth::Local(LocalUserAuth { ref password_hash }) => {
                (None, None, Some(password_hash))
            }
        };

        let new_id = Uuid::new_v4();
        sqlx::query!(
            r#"
                INSERT INTO user_auth
                  (
                    id,
                    user_id,
                    kind,
                    auth_user_id,
                    auth_id_token,
                    password_hash
                  )
                VALUES
                  (
                    $1,
                    $2,
                    $3::user_auth_kind,
                    $4,
                    $5,
                    $6
                  )
            "#,
            new_id,
            user_id.0,
            user.auth.to_string() as _,
            auth_user_id.map(|id| id.to_string()),
            auth_id_token.map(|token| token.to_string()),
            password_hash.map(|hash| hash.expose_secret().0.to_string()),
        )
        .execute(&mut **executor)
        .await
        .with_context(|| format!("Failed to user auth paramters for user {user_id}"))?;

        Ok(User {
            id: user_id,
            ..user
        })
    }

    #[tracing::instrument(level = "debug", skip(self, executor, auth_id_token))]
    async fn update_user_auth_id_token<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        auth_user_id: &AuthUserId,
        auth_id_token: &AuthIdToken,
    ) -> Result<UpdateStatus<User>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new("UPDATE user_auth SET");
        query_builder
            .push(" auth_id_token = ")
            .push_bind(auth_id_token.to_string())
            .push(r#" FROM "user" "#)
            .push(" WHERE ");
        let mut separated = query_builder.separated(" AND ");
        separated.push(r#" user_auth.user_id = "user".id "#);
        separated
            .push(" user_auth.auth_user_id = ")
            .push_bind_unseparated(auth_user_id.to_string());

        query_builder
            .push(
                r#"
                RETURNING
                  "user".id,
                  "user".first_name,
                  "user".last_name,
                  "user".email,
                  "user".created_at,
                  "user".updated_at,
                  user_auth.kind as user_auth_kind,
                  user_auth.auth_user_id,
                  user_auth.auth_id_token,
                  user_auth.password_hash,
                  (SELECT"#,
            )
            .push(" auth_id_token != ")
            .push_bind(auth_id_token.to_string())
            .push(" FROM user_auth WHERE auth_user_id = ")
            .push_bind(auth_user_id.to_string())
            .push(r#") as "is_updated""#);

        let record: Option<UpdatedUserRow> = query_builder
            .build_query_as::<UpdatedUserRow>()
            .fetch_optional(&mut **executor)
            .await
            .context(format!(
                "Failed to update user with auth ID {auth_user_id} from storage"
            ))?;

        if let Some(updated_user_row) = record {
            Ok(UpdateStatus {
                updated: updated_user_row.is_updated,
                result: Some(updated_user_row.user_row.try_into().unwrap()),
            })
        } else {
            Ok(UpdateStatus {
                updated: false,
                result: None,
            })
        }
    }
}

#[derive(sqlx::Type, Debug)]
#[sqlx(type_name = "user_auth_kind")]
enum PgUserAuthKind {
    OpenIdConnect,
    Local,
}

#[derive(Debug, sqlx::FromRow)]
pub struct UserRow {
    id: Uuid,
    first_name: String,
    last_name: String,
    email: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    user_auth_kind: PgUserAuthKind,
    auth_user_id: Option<String>,
    auth_id_token: Option<String>,
    password_hash: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct UpdatedUserRow {
    #[sqlx(flatten)]
    pub user_row: UserRow,
    pub is_updated: bool,
}

impl TryFrom<UserRow> for User {
    type Error = UniversalInboxError;

    fn try_from(row: UserRow) -> Result<Self, Self::Error> {
        (&row).try_into()
    }
}

impl TryFrom<&UserRow> for User {
    type Error = UniversalInboxError;

    fn try_from(row: &UserRow) -> Result<Self, Self::Error> {
        let auth = match row.user_auth_kind {
            PgUserAuthKind::Local => UserAuth::Local(LocalUserAuth {
                password_hash: Secret::new(PasswordHash(row.password_hash.clone().context(
                    "Expected to find password hash in storage with local authentication",
                )?)),
            }),
            PgUserAuthKind::OpenIdConnect => UserAuth::OpenIdConnect(OpenIdConnectUserAuth {
                auth_user_id: AuthUserId(row.auth_user_id.clone().context(
                    "Expected to find OIDC user ID in storage with OIDC authentication",
                )?),
                auth_id_token: AuthIdToken(row.auth_id_token.clone().context(
                    "Expected to find OIDC ID token in storage with OIDC authentication",
                )?),
            }),
        };

        Ok(User {
            id: row.id.into(),
            first_name: row.first_name.clone(),
            last_name: row.last_name.clone(),
            email: row
                .email
                .parse()
                .context("Unable to parse stored email address")?,
            auth,
            created_at: DateTime::from_naive_utc_and_offset(row.created_at, Utc),
            updated_at: DateTime::from_naive_utc_and_offset(row.updated_at, Utc),
        })
    }
}
