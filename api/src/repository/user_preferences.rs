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
                    open_links_in_background,
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
        // For each field: a value bind + a "provided" flag bind. The flag drives
        // the ON CONFLICT CASE so an absent patch field keeps the existing value.
        // $2/$3: default_task_manager_provider_kind value / provided
        let (value, provided) = match &patch.default_task_manager_provider_kind {
            Some(opt) => (opt.as_ref().map(|kind| kind.to_string()), true),
            None => (None, false),
        };
        // $4/$5: open_links_in_background value / provided. The value defaults to
        // false when absent so a fresh INSERT gets the NOT NULL column's default.
        let (open_links_in_background, open_links_in_background_provided) =
            match patch.open_links_in_background {
                Some(value) => (value, true),
                None => (false, false),
            };

        let row = sqlx::query(
            r#"
                INSERT INTO user_preferences (
                    user_id,
                    default_task_manager_provider_kind,
                    open_links_in_background
                )
                VALUES ($1, $2, $4)
                ON CONFLICT (user_id)
                DO UPDATE SET
                    default_task_manager_provider_kind = CASE
                        WHEN $3 THEN $2
                        ELSE user_preferences.default_task_manager_provider_kind
                    END,
                    open_links_in_background = CASE
                        WHEN $5 THEN $4
                        ELSE user_preferences.open_links_in_background
                    END,
                    updated_at = NOW()
                RETURNING
                    user_id,
                    default_task_manager_provider_kind,
                    open_links_in_background,
                    created_at,
                    updated_at
            "#,
        )
        .bind(user_id.0)
        .bind(value)
        .bind(provided)
        .bind(open_links_in_background)
        .bind(open_links_in_background_provided)
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
    let open_links_in_background: bool = row.get("open_links_in_background");
    let created_at: DateTime<Utc> = row.get("created_at");
    let updated_at: DateTime<Utc> = row.get("updated_at");

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
        open_links_in_background,
        created_at,
        updated_at,
    })
}
