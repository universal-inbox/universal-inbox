use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Postgres, Row, Transaction};

use universal_inbox::{
    integration_connection::provider::IntegrationProviderKind,
    user::{UserId, UserPreferences, UserPreferencesPatch},
};

use crate::{repository::Repository, universal_inbox::UniversalInboxError};

#[async_trait]
pub trait UserPreferencesRepository {
    async fn get_user_preferences(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Option<UserPreferences>, UniversalInboxError>;

    async fn create_or_update_user_preferences(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        patch: &UserPreferencesPatch,
    ) -> Result<UserPreferences, UniversalInboxError>;
}

#[async_trait]
impl UserPreferencesRepository for Repository {
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = user_id.to_string()),
        err
    )]
    async fn get_user_preferences(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Option<UserPreferences>, UniversalInboxError> {
        let row = sqlx::query(
            r#"
                SELECT
                    user_id,
                    default_task_manager_provider_kind,
                    created_at,
                    updated_at
                FROM user_preferences
                WHERE user_id = $1
            "#,
        )
        .bind(user_id.0)
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| {
            let message =
                format!("Failed to fetch user preferences for user {user_id} from storage: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        row.map(|r| user_preferences_from_row(&r)).transpose()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = user_id.to_string()),
        err
    )]
    async fn create_or_update_user_preferences(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        patch: &UserPreferencesPatch,
    ) -> Result<UserPreferences, UniversalInboxError> {
        // $2: the value (None means NULL in DB)
        // $3: whether the field was provided in the patch (true = update it, false = keep existing)
        let (value, provided) = match &patch.default_task_manager_provider_kind {
            Some(opt) => (opt.as_ref().map(|kind| kind.to_string()), true),
            None => (None, false),
        };

        let row = sqlx::query(
            r#"
                INSERT INTO user_preferences (user_id, default_task_manager_provider_kind)
                VALUES ($1, $2)
                ON CONFLICT (user_id)
                DO UPDATE SET
                    default_task_manager_provider_kind = CASE
                        WHEN $3 THEN $2
                        ELSE user_preferences.default_task_manager_provider_kind
                    END,
                    updated_at = NOW()
                RETURNING
                    user_id,
                    default_task_manager_provider_kind,
                    created_at,
                    updated_at
            "#,
        )
        .bind(user_id.0)
        .bind(value)
        .bind(provided)
        .fetch_one(&mut **executor)
        .await
        .map_err(|err| {
            let message =
                format!("Failed to create or update user preferences for user {user_id}: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        user_preferences_from_row(&row)
    }
}

fn user_preferences_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<UserPreferences, UniversalInboxError> {
    let user_id: uuid::Uuid = row.get("user_id");
    let default_task_manager_provider_kind: Option<String> =
        row.get("default_task_manager_provider_kind");
    let created_at: chrono::NaiveDateTime = row.get("created_at");
    let updated_at: chrono::NaiveDateTime = row.get("updated_at");

    let default_task_manager_provider_kind = default_task_manager_provider_kind
        .map(|kind| {
            IntegrationProviderKind::from_str(&kind).map_err(|_| {
                UniversalInboxError::InvalidInputData {
                    source: None,
                    user_error: format!("Invalid default_task_manager_provider_kind value: {kind}"),
                }
            })
        })
        .transpose()?;

    Ok(UserPreferences {
        user_id: user_id.into(),
        default_task_manager_provider_kind,
        created_at: DateTime::from_naive_utc_and_offset(created_at, Utc),
        updated_at: DateTime::from_naive_utc_and_offset(updated_at, Utc),
    })
}
