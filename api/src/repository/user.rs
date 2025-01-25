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
        AuthUserId, EmailValidationToken, LocalUserAuth, OpenIdConnectUserAuth, PasswordHash,
        PasswordResetToken, User, UserAuth, UserId,
    },
};

use crate::universal_inbox::{UniversalInboxError, UpdateStatus};

use super::Repository;

#[async_trait]
pub trait UserRepository {
    async fn get_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: UserId,
    ) -> Result<Option<User>, UniversalInboxError>;

    async fn fetch_all_users(
        &self,
        executor: &mut Transaction<'_, Postgres>,
    ) -> Result<Vec<User>, UniversalInboxError>;

    async fn get_user_by_auth_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        auth_user_id: AuthUserId,
    ) -> Result<Option<User>, UniversalInboxError>;

    async fn get_user_by_email(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        email: &EmailAddress,
    ) -> Result<Option<User>, UniversalInboxError>;

    async fn create_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user: User,
    ) -> Result<User, UniversalInboxError>;

    async fn update_user_auth_id_token(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        auth_user_id: &AuthUserId,
        auth_id_token: &AuthIdToken,
    ) -> Result<UpdateStatus<User>, UniversalInboxError>;

    async fn update_email_validation_parameters(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        email_validated_at: Option<DateTime<Utc>>,
        email_validation_sent_at: Option<DateTime<Utc>>,
        email_validation_token: Option<EmailValidationToken>,
    ) -> Result<UpdateStatus<User>, UniversalInboxError>;

    async fn get_user_email_validation_token(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Option<EmailValidationToken>, UniversalInboxError>;

    async fn update_password_reset_parameters(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        email_address: EmailAddress,
        password_reset_sent_at: Option<DateTime<Utc>>,
        password_reset_token: Option<PasswordResetToken>,
    ) -> Result<UpdateStatus<User>, UniversalInboxError>;

    async fn update_password(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        password_hash: Secret<PasswordHash>,
        password_reset_token: Option<PasswordResetToken>,
    ) -> Result<UpdateStatus<User>, UniversalInboxError>;

    async fn get_password_reset_token(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Option<PasswordResetToken>, UniversalInboxError>;
}

#[async_trait]
impl UserRepository for Repository {
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = id.to_string()),
        err
    )]
    async fn get_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
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
                  "user".email_validated_at,
                  "user".email_validation_sent_at,
                  "user".created_at,
                  "user".updated_at,
                  user_auth.kind as "user_auth_kind: _",
                  user_auth.auth_user_id,
                  user_auth.auth_id_token,
                  user_auth.password_hash,
                  user_auth.password_reset_at,
                  user_auth.password_reset_sent_at
                FROM "user"
                INNER JOIN user_auth ON user_auth.user_id = "user".id
                WHERE "user".id = $1
            "#,
            id.0
        )
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!("Failed to fetch user {id} from storage: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        row.map(|user_row| user_row.try_into()).transpose()
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn fetch_all_users(
        &self,
        executor: &mut Transaction<'_, Postgres>,
    ) -> Result<Vec<User>, UniversalInboxError> {
        let rows = sqlx::query_as!(
            UserRow,
            r#"
                SELECT
                  "user".id,
                  "user".first_name,
                  "user".last_name,
                  "user".email,
                  "user".email_validated_at,
                  "user".email_validation_sent_at,
                  "user".created_at,
                  "user".updated_at,
                  user_auth.kind as "user_auth_kind: _",
                  user_auth.auth_user_id,
                  user_auth.auth_id_token,
                  user_auth.password_hash,
                  user_auth.password_reset_at,
                  user_auth.password_reset_sent_at
                FROM "user"
                INNER JOIN user_auth ON user_auth.user_id = "user".id
            "#
        )
        .fetch_all(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!("Failed to fetch all users from storage: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        rows.iter().map(|r| r.try_into()).collect()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(auth_user_id = auth_user_id.to_string()),
        err
    )]
    async fn get_user_by_auth_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
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
                  "user".email_validated_at,
                  "user".email_validation_sent_at,
                  "user".created_at,
                  "user".updated_at,
                  user_auth.kind as "user_auth_kind: _",
                  user_auth.auth_user_id,
                  user_auth.auth_id_token,
                  user_auth.password_hash,
                  user_auth.password_reset_at,
                  user_auth.password_reset_sent_at
                FROM "user"
                INNER JOIN user_auth ON user_auth.user_id = "user".id
                WHERE user_auth.auth_user_id = $1
            "#,
            auth_user_id.0
        )
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!(
                "Failed to fetch user with auth provider user ID {auth_user_id} from storage: {err}"
            );
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        row.map(|user_row| user_row.try_into()).transpose()
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    async fn get_user_by_email(
        &self,
        executor: &mut Transaction<'_, Postgres>,
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
                  "user".email_validated_at,
                  "user".email_validation_sent_at,
                  "user".created_at,
                  "user".updated_at,
                  user_auth.kind as "user_auth_kind: _",
                  user_auth.auth_user_id,
                  user_auth.auth_id_token,
                  user_auth.password_hash,
                  user_auth.password_reset_at,
                  user_auth.password_reset_sent_at
                FROM "user"
                INNER JOIN user_auth ON user_auth.user_id = "user".id
                WHERE "user".email = $1
            "#,
            email.to_string()
        )
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!("Failed to fetch user by email from storage: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        row.map(|user_row| user_row.try_into()).transpose()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = user.id.to_string()),
        err
    )]
    async fn create_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
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
                    _ => UniversalInboxError::Unexpected(anyhow!("Failed to create new user: {e}")),
                }
            })?,
        );

        let (auth_user_id, auth_id_token, password_hash) = match user.auth {
            UserAuth::OIDCAuthorizationCodePKCE(OpenIdConnectUserAuth {
                ref auth_user_id,
                ref auth_id_token,
            })
            | UserAuth::OIDCGoogleAuthorizationCode(OpenIdConnectUserAuth {
                ref auth_user_id,
                ref auth_id_token,
            }) => (Some(auth_user_id), Some(auth_id_token), None),
            UserAuth::Local(LocalUserAuth {
                ref password_hash, ..
            }) => (None, None, Some(password_hash)),
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
        .map_err(|err| {
            let message = format!("Failed to user auth paramters for user {user_id}: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        Ok(User {
            id: user_id,
            ..user
        })
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            auth_user_id = auth_user_id.to_string(),
            auth_id_token = auth_id_token.to_string()
        ),
        err
    )]
    async fn update_user_auth_id_token(
        &self,
        executor: &mut Transaction<'_, Postgres>,
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
                  "user".email_validated_at,
                  "user".email_validation_sent_at,
                  "user".created_at,
                  "user".updated_at,
                  user_auth.kind as user_auth_kind,
                  user_auth.auth_user_id,
                  user_auth.auth_id_token,
                  user_auth.password_hash,
                  user_auth.password_reset_at,
                  user_auth.password_reset_sent_at,
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
            .map_err(|err| {
                let message = format!(
                    "Failed to update user with auth ID {auth_user_id} from storage: {err}"
                );
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

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

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            user.id = user_id.to_string(),
            email_validated_at = email_validated_at.map(|d| d.to_rfc3339()),
            email_validation_sent_at = email_validation_sent_at.map(|d| d.to_rfc3339()),
        ),
        err
    )]
    async fn update_email_validation_parameters(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        email_validated_at: Option<DateTime<Utc>>,
        email_validation_sent_at: Option<DateTime<Utc>>,
        email_validation_token: Option<EmailValidationToken>,
    ) -> Result<UpdateStatus<User>, UniversalInboxError> {
        if email_validated_at.is_none()
            && email_validation_sent_at.is_none()
            && email_validation_token.is_none()
        {
            return Ok(UpdateStatus {
                updated: false,
                result: None,
            });
        }

        let mut query_builder = QueryBuilder::new(r#"UPDATE "user" SET"#);
        let mut separated = query_builder.separated(", ");
        if let Some(email_validated_at) = &email_validated_at {
            separated
                .push(" email_validated_at = ")
                .push_bind_unseparated(email_validated_at.naive_utc());
        }
        if let Some(email_validation_sent_at) = &email_validation_sent_at {
            separated
                .push(" email_validation_sent_at = ")
                .push_bind_unseparated(email_validation_sent_at.naive_utc());
        }
        if let Some(email_validation_token) = &email_validation_token {
            separated
                .push(" email_validation_token = ")
                .push_bind_unseparated(email_validation_token.0);
        }
        query_builder.push(" FROM user_auth WHERE ");
        let mut separated = query_builder.separated(" AND ");
        separated
            .push(r#"user_auth.user_id = "user".id"#)
            .push(r#""user".id = "#)
            .push_bind_unseparated(user_id.0);

        query_builder.push(
            r#"
                RETURNING
                  "user".id,
                  "user".first_name,
                  "user".last_name,
                  "user".email,
                  "user".email_validated_at,
                  "user".email_validation_sent_at,
                  "user".created_at,
                  "user".updated_at,
                  user_auth.kind as user_auth_kind,
                  user_auth.auth_user_id,
                  user_auth.auth_id_token,
                  user_auth.password_hash,
                  user_auth.password_reset_at,
                  user_auth.password_reset_sent_at,
                  (SELECT"#,
        );
        let mut separated = query_builder.separated(" OR ");
        if let Some(email_validated_at) = &email_validated_at {
            separated
                .push(" (email_validated_at is NULL OR email_validated_at != ")
                .push_bind_unseparated(email_validated_at.naive_utc())
                .push_unseparated(")");
        }
        if let Some(email_validation_sent_at) = &email_validation_sent_at {
            separated
                .push(" (email_validation_sent_at is NULL OR email_validation_sent_at != ")
                .push_bind_unseparated(email_validation_sent_at.naive_utc())
                .push_unseparated(")");
        }
        if let Some(email_validation_token) = &email_validation_token {
            separated
                .push(" (email_validation_token is NULL OR email_validation_token != ")
                .push_bind_unseparated(email_validation_token.0)
                .push_unseparated(")");
        }
        query_builder
            .push(r#" FROM "user" WHERE id = "#)
            .push_bind(user_id.0)
            .push(r#") as "is_updated""#);

        let record: Option<UpdatedUserRow> = query_builder
            .build_query_as::<UpdatedUserRow>()
            .fetch_optional(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!(
                    "Failed to update user email validation parameter with ID {user_id} from storage: {err}"
                );
                UniversalInboxError::DatabaseError { source: err, message }
            })?;

        if let Some(updated_user_row) = record {
            Ok(UpdateStatus {
                updated: updated_user_row.is_updated,
                result: Some(updated_user_row.user_row.try_into()?),
            })
        } else {
            Ok(UpdateStatus {
                updated: false,
                result: None,
            })
        }
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = user_id.to_string()),
        err
    )]
    async fn get_user_email_validation_token(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Option<EmailValidationToken>, UniversalInboxError> {
        let row: Option<Option<Uuid>> = sqlx::query_scalar!(
            r#"SELECT email_validation_token FROM "user" WHERE id = $1"#,
            user_id.0
        )
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!(
                "Failed to fetch user email validation token for user ID {user_id} from storage: {err}"
            );
            UniversalInboxError::DatabaseError { source: err, message }
        })?;

        Ok(row.and_then(|row| row.map(|token| token.into())))
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            email_address = email_address.as_str(),
            password_reset_sent_at = password_reset_sent_at.map(|d| d.to_rfc3339()),
        ),
        err
    )]
    async fn update_password_reset_parameters(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        email_address: EmailAddress,
        password_reset_sent_at: Option<DateTime<Utc>>,
        password_reset_token: Option<PasswordResetToken>,
    ) -> Result<UpdateStatus<User>, UniversalInboxError> {
        if password_reset_sent_at.is_none() && password_reset_token.is_none() {
            return Ok(UpdateStatus {
                updated: false,
                result: None,
            });
        }

        let mut query_builder = QueryBuilder::new("UPDATE user_auth SET");
        let mut separated = query_builder.separated(", ");
        if let Some(password_reset_sent_at) = &password_reset_sent_at {
            separated
                .push(" password_reset_sent_at = ")
                .push_bind_unseparated(password_reset_sent_at.naive_utc());
        }
        if let Some(password_reset_token) = &password_reset_token {
            separated
                .push(" password_reset_token = ")
                .push_bind_unseparated(password_reset_token.0);
        }
        query_builder.push(r#" FROM "user" WHERE "#);
        let mut separated = query_builder.separated(" AND ");
        separated
            .push(r#"user_auth.user_id = "user".id"#)
            .push(r#""user".email = "#)
            .push_bind_unseparated(email_address.as_str());

        query_builder.push(
            r#"
                RETURNING
                  "user".id,
                  "user".first_name,
                  "user".last_name,
                  "user".email,
                  "user".email_validated_at,
                  "user".email_validation_sent_at,
                  "user".created_at,
                  "user".updated_at,
                  user_auth.kind as user_auth_kind,
                  user_auth.auth_user_id,
                  user_auth.auth_id_token,
                  user_auth.password_hash,
                  user_auth.password_reset_at,
                  user_auth.password_reset_sent_at,
                  (SELECT true) as is_updated
            "#,
        );

        let record: Option<UpdatedUserRow> = query_builder
            .build_query_as::<UpdatedUserRow>()
            .fetch_optional(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!(
                    "Failed to update password parameters with for user with email {email_address} from storage: {err}"
                );
                UniversalInboxError::DatabaseError { source: err, message }
            })?;

        if let Some(updated_user_row) = record {
            Ok(UpdateStatus {
                updated: true,
                result: Some(updated_user_row.user_row.try_into()?),
            })
        } else {
            Ok(UpdateStatus {
                updated: false,
                result: None,
            })
        }
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = user_id.to_string()),
        err
    )]
    async fn update_password(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        password_hash: Secret<PasswordHash>,
        password_reset_token: Option<PasswordResetToken>,
    ) -> Result<UpdateStatus<User>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new("UPDATE user_auth SET");
        let mut separated = query_builder.separated(", ");
        separated
            .push(" password_hash = ")
            .push_bind_unseparated(password_hash.expose_secret().0.as_str())
            .push(" password_reset_at = ")
            .push_bind_unseparated(Utc::now().naive_utc())
            .push(" password_reset_token = NULL ");
        query_builder.push(r#" FROM "user" WHERE "#);
        let mut separated = query_builder.separated(" AND ");
        separated
            .push(r#"user_auth.user_id = "user".id"#)
            .push(r#""user".id = "#)
            .push_bind_unseparated(user_id.0);

        // If a password reset token was provided, we need to ensure that it matches the one in the database
        // If no token is provider, it means that the user is changing their password without having requested a reset
        if let Some(password_reset_token) = &password_reset_token {
            separated
                .push("user_auth.password_reset_token = ")
                .push_bind_unseparated(password_reset_token.0);
        }

        query_builder.push(
            r#"
                RETURNING
                  "user".id,
                  "user".first_name,
                  "user".last_name,
                  "user".email,
                  "user".email_validated_at,
                  "user".email_validation_sent_at,
                  "user".created_at,
                  "user".updated_at,
                  user_auth.kind as user_auth_kind,
                  user_auth.auth_user_id,
                  user_auth.auth_id_token,
                  user_auth.password_hash,
                  user_auth.password_reset_at,
                  user_auth.password_reset_sent_at,
                  (SELECT "#,
        );
        let mut separated = query_builder.separated(" OR ");
        separated
            .push("password_reset_token is not NULL")
            .push("password_hash != ")
            .push_bind_unseparated(password_hash.expose_secret().0.as_str());
        query_builder
            .push(" FROM user_auth WHERE user_id = ")
            .push_bind(user_id.0)
            .push(r#") as "is_updated""#);

        let record: Option<UpdatedUserRow> = query_builder
            .build_query_as::<UpdatedUserRow>()
            .fetch_optional(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!(
                    "Failed to update password for user {} from storage: {err}",
                    user_id.0
                );
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        if let Some(updated_user_row) = record {
            Ok(UpdateStatus {
                updated: updated_user_row.is_updated,
                result: Some(updated_user_row.user_row.try_into()?),
            })
        } else {
            Ok(UpdateStatus {
                updated: false,
                result: None,
            })
        }
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = user_id.to_string()),
        err
    )]
    async fn get_password_reset_token(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Option<PasswordResetToken>, UniversalInboxError> {
        let row: Option<Option<Uuid>> = sqlx::query_scalar!(
            "SELECT password_reset_token FROM user_auth WHERE user_id = $1",
            user_id.0
        )
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!(
                "Failed to fetch password reset token for user ID {user_id} from storage: {err}"
            );
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        Ok(row.and_then(|row| row.map(|token| token.into())))
    }
}

#[derive(sqlx::Type, Debug)]
#[sqlx(type_name = "user_auth_kind")]
enum PgUserAuthKind {
    OIDCAuthorizationCodePKCE,
    OIDCGoogleAuthorizationCode,
    Local,
}

#[derive(Debug, sqlx::FromRow)]
pub struct UserRow {
    id: Uuid,
    first_name: Option<String>,
    last_name: Option<String>,
    email: String,
    email_validated_at: Option<NaiveDateTime>,
    email_validation_sent_at: Option<NaiveDateTime>,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    user_auth_kind: PgUserAuthKind,
    auth_user_id: Option<String>,
    auth_id_token: Option<String>,
    password_hash: Option<String>,
    password_reset_at: Option<NaiveDateTime>,
    password_reset_sent_at: Option<NaiveDateTime>,
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
                password_reset_at: row
                    .password_reset_at
                    .map(|naive| DateTime::from_naive_utc_and_offset(naive, Utc)),
                password_reset_sent_at: row
                    .password_reset_sent_at
                    .map(|naive| DateTime::from_naive_utc_and_offset(naive, Utc)),
            }),
            PgUserAuthKind::OIDCAuthorizationCodePKCE => UserAuth::OIDCAuthorizationCodePKCE(OpenIdConnectUserAuth {
                auth_user_id: AuthUserId(row.auth_user_id.clone().context(
                    "Expected to find OIDC user ID in storage with OIDCAuthorizationCodePKCE authentication",
                )?),
                auth_id_token: AuthIdToken(row.auth_id_token.clone().context(
                    "Expected to find OIDC ID token in storage with OIDCAuthorizationCodePKCE authentication",
                )?),
            }),
            PgUserAuthKind::OIDCGoogleAuthorizationCode => UserAuth::OIDCGoogleAuthorizationCode(OpenIdConnectUserAuth {
                auth_user_id: AuthUserId(row.auth_user_id.clone().context(
                    "Expected to find OIDC user ID in storage with OIDCGoogleAuthorizationCode authentication",
                )?),
                auth_id_token: AuthIdToken(row.auth_id_token.clone().context(
                    "Expected to find OIDC ID token in storage with OIDCGoogleAuthorizationCode authentication",
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
            email_validated_at: row
                .email_validated_at
                .map(|naive| DateTime::from_naive_utc_and_offset(naive, Utc)),
            email_validation_sent_at: row
                .email_validation_sent_at
                .map(|naive| DateTime::from_naive_utc_and_offset(naive, Utc)),
            auth,
            created_at: DateTime::from_naive_utc_and_offset(row.created_at, Utc),
            updated_at: DateTime::from_naive_utc_and_offset(row.updated_at, Utc),
        })
    }
}
