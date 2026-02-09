use anyhow::{Context, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use email_address::EmailAddress;
use secrecy::{ExposeSecret, SecretBox};
use sqlx::{Postgres, QueryBuilder, Transaction, types::Json};
use uuid::Uuid;
use webauthn_rs::prelude::*;

use universal_inbox::{
    auth::AuthIdToken,
    user::{EmailValidationToken, PasswordHash, PasswordResetToken, User, UserId, Username},
};

use crate::{
    repository::Repository,
    universal_inbox::{
        UniversalInboxError, UpdateStatus,
        user::model::{
            AuthUserId, LocalUserAuth, OpenIdConnectUserAuth, PasskeyUserAuth, UserAuth,
        },
    },
};

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

    async fn fetch_all_users_and_auth(
        &self,
        executor: &mut Transaction<'_, Postgres>,
    ) -> Result<Vec<(User, UserAuth)>, UniversalInboxError>;

    async fn delete_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<bool, UniversalInboxError>;

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
        user_auth: UserAuth,
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
        password_hash: SecretBox<PasswordHash>,
        password_reset_token: Option<PasswordResetToken>,
    ) -> Result<UpdateStatus<User>, UniversalInboxError>;

    async fn get_password_reset_token(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Option<PasswordResetToken>, UniversalInboxError>;

    async fn get_user_auth_by_email(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_email: &EmailAddress,
    ) -> Result<Option<(UserAuth, UserId)>, UniversalInboxError>;

    async fn get_user_auth(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Option<UserAuth>, UniversalInboxError>;

    async fn get_user_auth_by_username(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        username: &Username,
    ) -> Result<Option<(UserAuth, UserId)>, UniversalInboxError>;

    async fn update_passkey(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: &UserId,
        passkey: &Passkey,
    ) -> Result<UpdateStatus<User>, UniversalInboxError>;
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
                  "user".is_testing,
                  "user".created_at,
                  "user".updated_at
                FROM "user"
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
                  "user".is_testing,
                  "user".created_at,
                  "user".updated_at
                FROM "user"
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

    #[tracing::instrument(level = "debug", skip_all)]
    async fn fetch_all_users_and_auth(
        &self,
        executor: &mut Transaction<'_, Postgres>,
    ) -> Result<Vec<(User, UserAuth)>, UniversalInboxError> {
        let rows: Vec<UserAndUserAuthRow> = QueryBuilder::new(
            r#"
                SELECT
                  "user".id,
                  "user".first_name,
                  "user".last_name,
                  "user".email,
                  "user".email_validated_at,
                  "user".email_validation_sent_at,
                  "user".is_testing,
                  "user".created_at,
                  "user".updated_at,
                  user_auth.kind,
                  user_auth.password_hash,
                  user_auth.password_reset_at,
                  user_auth.password_reset_sent_at,
                  user_auth.auth_user_id,
                  user_auth.auth_id_token,
                  user_auth.username,
                  user_auth.passkey,
                  user_auth.user_id
                FROM "user"
                INNER JOIN user_auth ON user_auth.user_id = "user".id
            "#,
        )
        .build_query_as::<UserAndUserAuthRow>()
        .fetch_all(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!("Failed to fetch all users from storage: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        rows.iter()
            .map(|r| {
                let user: Result<User, UniversalInboxError> = r.try_into();
                let user_auth: Result<UserAuth, UniversalInboxError> = r.try_into();
                user.and_then(|u| user_auth.map(|a| (u, a)))
            })
            .collect()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = user_id.to_string())
    )]
    async fn delete_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<bool, UniversalInboxError> {
        let res = sqlx::query!(r#"DELETE FROM "user" WHERE id = $1"#, user_id.0)
            .execute(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to delete user {user_id} from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        Ok(res.rows_affected() == 1)
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
                  "user".is_testing,
                  "user".created_at,
                  "user".updated_at
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
                  "user".is_testing,
                  "user".created_at,
                  "user".updated_at
                FROM "user"
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
        user_auth: UserAuth,
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
                    is_testing,
                    created_at,
                    updated_at
                  )
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                RETURNING
                  id
            "#,
                user.id.0,
                user.first_name,
                user.last_name,
                user.email.as_ref().map(|email| email.to_string()),
                user.is_testing,
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
                        source: Some(e),
                        id: user.id.0,
                    },
                    _ => UniversalInboxError::Unexpected(anyhow!("Failed to create new user: {e}")),
                }
            })?,
        );

        let (auth_user_id, auth_id_token, password_hash, username, passkey) = match user_auth {
            UserAuth::OIDCAuthorizationCodePKCE(ref oidc_user_auth)
            | UserAuth::OIDCGoogleAuthorizationCode(ref oidc_user_auth) => (
                Some(oidc_user_auth.auth_user_id.clone()),
                Some(oidc_user_auth.auth_id_token.clone()),
                None,
                None,
                None,
            ),
            UserAuth::Local(ref local_user_auth) => (
                None,
                None,
                Some(local_user_auth.password_hash.clone()),
                None,
                None,
            ),
            UserAuth::Passkey(ref passkey_user_auth) => (
                None,
                None,
                None,
                Some(passkey_user_auth.username.clone()),
                Some(passkey_user_auth.passkey.clone()),
            ),
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
                    password_hash,
                    username,
                    passkey
                  )
                VALUES
                  (
                    $1,
                    $2,
                    $3::user_auth_kind,
                    $4,
                    $5,
                    $6,
                    $7,
                    $8
                  )
            "#,
            new_id,
            user_id.0,
            user_auth.to_string() as _,
            auth_user_id.map(|id| id.to_string()),
            auth_id_token.map(|token| token.to_string()),
            password_hash.map(|hash| hash.expose_secret().0.to_string()),
            username.map(|username| username.to_string()),
            passkey.map(|passkey| Json(passkey.clone())) as Option<Json<Passkey>>,
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
                  "user".is_testing,
                  "user".created_at,
                  "user".updated_at,
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
                  "user".is_testing,
                  "user".created_at,
                  "user".updated_at,
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
                  "user".is_testing,
                  "user".created_at,
                  "user".updated_at,
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
        password_hash: SecretBox<PasswordHash>,
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
                  "user".is_testing,
                  "user".created_at,
                  "user".updated_at,
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

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.email = user_email.to_string()),
        err
    )]
    async fn get_user_auth_by_email(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_email: &EmailAddress,
    ) -> Result<Option<(UserAuth, UserId)>, UniversalInboxError> {
        let row: Option<UserAuthRow> = sqlx::query_as!(
            UserAuthRow,
            r#"
                SELECT
                    kind as "kind: _",
                    password_hash,
                    password_reset_at,
                    password_reset_sent_at,
                    auth_user_id,
                    auth_id_token,
                    username,
                    passkey as "passkey: Json<Passkey>",
                    user_id
                FROM user_auth
                JOIN "user" ON user_auth.user_id = "user".id
                WHERE "user".email = $1
            "#,
            user_email.to_string()
        )
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!(
                "Failed to fetch password hash for user with email {user_email} from storage: {err}"
            );
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        let Some(user_auth) = row else {
            return Ok(None);
        };
        let user_id = user_auth.user_id.into();
        Ok(Some((user_auth.try_into()?, user_id)))
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = user_id.to_string()),
        err
    )]
    async fn get_user_auth(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Option<UserAuth>, UniversalInboxError> {
        let row = sqlx::query_as!(
            UserAuthRow,
            r#"
                SELECT
                    kind as "kind: _",
                    password_hash,
                    password_reset_at,
                    password_reset_sent_at,
                    auth_user_id,
                    auth_id_token,
                    username,
                    passkey as "passkey: Json<Passkey>",
                    user_id
                FROM user_auth
                WHERE user_id = $1
            "#,
            user_id.0
        )
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| {
            let message =
                format!("Failed to fetch user auth for user {user_id} from storage: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        row.map(|user_auth| user_auth.try_into()).transpose()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(username = username.to_string()),
        err
    )]
    async fn get_user_auth_by_username(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        username: &Username,
    ) -> Result<Option<(UserAuth, UserId)>, UniversalInboxError> {
        let row = sqlx::query_as!(
            UserAuthRow,
            r#"
                SELECT
                    kind as "kind: _",
                    password_hash,
                    password_reset_at,
                    password_reset_sent_at,
                    auth_user_id,
                    auth_id_token,
                    username,
                    passkey as "passkey: Json<Passkey>",
                    user_id
                FROM user_auth
                WHERE username = $1
            "#,
            username.0
        )
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| {
            let message =
                format!("Failed to fetch user auth for username {username} from storage: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        let Some(user_auth) = row else {
            return Ok(None);
        };
        let user_id = user_auth.user_id.into();
        Ok(Some((user_auth.try_into()?, user_id)))
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = user_id.to_string()),
        err
    )]
    async fn update_passkey(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: &UserId,
        passkey: &Passkey,
    ) -> Result<UpdateStatus<User>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new("UPDATE user_auth SET");
        query_builder
            .push(" passkey = ")
            .push_bind(Json(passkey))
            .push(
                r#"
                FROM "user"
                WHERE
                    user_auth.user_id = "user".id
                    AND user_auth.user_id = "#,
            )
            .push_bind(user_id.0);

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
                  "user".is_testing,
                  "user".created_at,
                  "user".updated_at,
                  (SELECT"#,
            )
            .push(" passkey != ")
            .push_bind(Json(passkey))
            .push(" FROM user_auth WHERE user_id = ")
            .push_bind(user_id.0)
            .push(r#") as "is_updated""#);

        let record: Option<UpdatedUserRow> = query_builder
            .build_query_as::<UpdatedUserRow>()
            .fetch_optional(&mut **executor)
            .await
            .map_err(|err| {
                let message =
                    format!("Failed to update user {user_id}'s passkey from storage: {err}");
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
}

#[derive(Debug, sqlx::FromRow)]
pub struct UserRow {
    id: Uuid,
    first_name: Option<String>,
    last_name: Option<String>,
    email: Option<String>,
    email_validated_at: Option<NaiveDateTime>,
    email_validation_sent_at: Option<NaiveDateTime>,
    is_testing: bool,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
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
        Ok(User {
            id: row.id.into(),
            first_name: row.first_name.clone(),
            last_name: row.last_name.clone(),
            email: row
                .email
                .as_ref()
                .map(|email| email.parse())
                .transpose()
                .context("Unable to parse stored email address")?,
            email_validated_at: row
                .email_validated_at
                .map(|naive| DateTime::from_naive_utc_and_offset(naive, Utc)),
            email_validation_sent_at: row
                .email_validation_sent_at
                .map(|naive| DateTime::from_naive_utc_and_offset(naive, Utc)),
            chat_support_email_signature: None,
            is_testing: row.is_testing,
            created_at: DateTime::from_naive_utc_and_offset(row.created_at, Utc),
            updated_at: DateTime::from_naive_utc_and_offset(row.updated_at, Utc),
        })
    }
}

#[derive(sqlx::Type, Debug)]
#[sqlx(type_name = "user_auth_kind")]
enum PgUserAuthKind {
    OIDCAuthorizationCodePKCE,
    OIDCGoogleAuthorizationCode,
    Local,
    Passkey,
}

#[derive(Debug, sqlx::FromRow)]
pub struct UserAuthRow {
    user_id: Uuid,
    kind: PgUserAuthKind,
    password_hash: Option<String>,
    password_reset_at: Option<NaiveDateTime>,
    password_reset_sent_at: Option<NaiveDateTime>,
    auth_user_id: Option<String>,
    auth_id_token: Option<String>,
    username: Option<String>,
    passkey: Option<Json<Passkey>>,
}

impl TryFrom<UserAuthRow> for UserAuth {
    type Error = UniversalInboxError;

    fn try_from(row: UserAuthRow) -> Result<Self, Self::Error> {
        (&row).try_into()
    }
}

impl TryFrom<&UserAuthRow> for UserAuth {
    type Error = UniversalInboxError;

    fn try_from(row: &UserAuthRow) -> Result<Self, Self::Error> {
        let auth = match row.kind {
            PgUserAuthKind::Local => UserAuth::Local(Box::new(LocalUserAuth {
                password_hash: SecretBox::new(Box::new(PasswordHash(row.password_hash.clone().context(
                    "Expected to find password hash in storage with local authentication",
                )?))),
                password_reset_at: row
                    .password_reset_at
                    .map(|naive| DateTime::from_naive_utc_and_offset(naive, Utc)),
                password_reset_sent_at: row
                    .password_reset_sent_at
                    .map(|naive| DateTime::from_naive_utc_and_offset(naive, Utc)),
            })),
            PgUserAuthKind::OIDCAuthorizationCodePKCE => UserAuth::OIDCAuthorizationCodePKCE(Box::new(OpenIdConnectUserAuth {
                auth_user_id: AuthUserId(row.auth_user_id.clone().context(
                    "Expected to find OIDC user ID in storage with OIDCAuthorizationCodePKCE authentication",
                )?),
                auth_id_token: AuthIdToken(row.auth_id_token.clone().context(
                    "Expected to find OIDC ID token in storage with OIDCAuthorizationCodePKCE authentication",
                )?),
            })),
            PgUserAuthKind::OIDCGoogleAuthorizationCode => UserAuth::OIDCGoogleAuthorizationCode(Box::new(OpenIdConnectUserAuth {
                auth_user_id: AuthUserId(row.auth_user_id.clone().context(
                    "Expected to find OIDC user ID in storage with OIDCGoogleAuthorizationCode authentication",
                )?),
                auth_id_token: AuthIdToken(row.auth_id_token.clone().context(
                    "Expected to find OIDC ID token in storage with OIDCGoogleAuthorizationCode authentication",
                )?),
            })),
            PgUserAuthKind::Passkey => UserAuth::Passkey(Box::new(PasskeyUserAuth  {
                username: Username (row.username.clone().context(
                    "Expected to find username in storage with Passkey authentication",
                )?),
                passkey: row.passkey.as_ref().map(|passkey| passkey.0.clone()).context(
                    "Expected to find passkey in storage with Passkey authentication",
                )?,
            })),
        };

        Ok(auth)
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct UserAndUserAuthRow {
    #[sqlx(flatten)]
    pub user_row: UserRow,
    #[sqlx(flatten)]
    pub user_auth_row: UserAuthRow,
}

impl TryFrom<&UserAndUserAuthRow> for User {
    type Error = UniversalInboxError;

    fn try_from(row: &UserAndUserAuthRow) -> Result<Self, Self::Error> {
        Ok(User {
            id: row.user_row.id.into(),
            first_name: row.user_row.first_name.clone(),
            last_name: row.user_row.last_name.clone(),
            email: row
                .user_row
                .email
                .as_ref()
                .map(|email| email.parse())
                .transpose()
                .context("Unable to parse stored email address")?,
            email_validated_at: row
                .user_row
                .email_validated_at
                .map(|naive| DateTime::from_naive_utc_and_offset(naive, Utc)),
            email_validation_sent_at: row
                .user_row
                .email_validation_sent_at
                .map(|naive| DateTime::from_naive_utc_and_offset(naive, Utc)),
            chat_support_email_signature: None,
            is_testing: row.user_row.is_testing,
            created_at: DateTime::from_naive_utc_and_offset(row.user_row.created_at, Utc),
            updated_at: DateTime::from_naive_utc_and_offset(row.user_row.updated_at, Utc),
        })
    }
}

impl TryFrom<&UserAndUserAuthRow> for UserAuth {
    type Error = UniversalInboxError;

    fn try_from(row: &UserAndUserAuthRow) -> Result<Self, Self::Error> {
        let auth = match row.user_auth_row.kind {
            PgUserAuthKind::Local => UserAuth::Local(Box::new(LocalUserAuth {
                password_hash: SecretBox::new(Box::new(PasswordHash(row.user_auth_row.password_hash.clone().context(
                    "Expected to find password hash in storage with local authentication",
                )?))),
                password_reset_at: row.user_auth_row
                    .password_reset_at
                    .map(|naive| DateTime::from_naive_utc_and_offset(naive, Utc)),
                password_reset_sent_at: row.user_auth_row
                    .password_reset_sent_at
                    .map(|naive| DateTime::from_naive_utc_and_offset(naive, Utc)),
            })),
            PgUserAuthKind::OIDCAuthorizationCodePKCE => UserAuth::OIDCAuthorizationCodePKCE(Box::new(OpenIdConnectUserAuth {
                auth_user_id: AuthUserId(row.user_auth_row.auth_user_id.clone().context(
                    "Expected to find OIDC user ID in storage with OIDCAuthorizationCodePKCE authentication",
                )?),
                auth_id_token: AuthIdToken(row.user_auth_row.auth_id_token.clone().context(
                    "Expected to find OIDC ID token in storage with OIDCAuthorizationCodePKCE authentication",
                )?),
            })),
            PgUserAuthKind::OIDCGoogleAuthorizationCode => UserAuth::OIDCGoogleAuthorizationCode(Box::new(OpenIdConnectUserAuth {
                auth_user_id: AuthUserId(row.user_auth_row.auth_user_id.clone().context(
                    "Expected to find OIDC user ID in storage with OIDCGoogleAuthorizationCode authentication",
                )?),
                auth_id_token: AuthIdToken(row.user_auth_row.auth_id_token.clone().context(
                    "Expected to find OIDC ID token in storage with OIDCGoogleAuthorizationCode authentication",
                )?),
            })),
            PgUserAuthKind::Passkey => UserAuth::Passkey(Box::new(PasskeyUserAuth  {
                username: Username (row.user_auth_row.username.clone().context(
                    "Expected to find username in storage with Passkey authentication",
                )?),
                passkey: row.user_auth_row.passkey.as_ref().map(|passkey| passkey.0.clone()).context(
                    "Expected to find passkey in storage with Passkey authentication",
                )?,
            })),
        };

        Ok(auth)
    }
}
