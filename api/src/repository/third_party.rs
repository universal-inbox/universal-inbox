use anyhow::anyhow;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::{postgres::PgRow, types::Json, Postgres, QueryBuilder, Row, Transaction};
use tracing::debug;
use uuid::Uuid;

use universal_inbox::{
    task::TaskSourceKind,
    third_party::item::{ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemId, ThirdPartyItemKind},
    user::UserId,
};

use crate::{
    repository::Repository,
    universal_inbox::{UniversalInboxError, UpsertStatus},
};

use super::FromRowWithPrefix;

#[async_trait]
pub trait ThirdPartyItemRepository {
    async fn create_or_update_third_party_item<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        third_party_item: Box<ThirdPartyItem>,
    ) -> Result<UpsertStatus<Box<ThirdPartyItem>>, UniversalInboxError>;

    async fn get_stale_task_source_third_party_items<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        active_task_source_third_party_item_ids: Vec<ThirdPartyItemId>,
        task_source_kind: TaskSourceKind,
        user_id: UserId,
    ) -> Result<Vec<ThirdPartyItem>, UniversalInboxError>;

    async fn has_third_party_item_for_source_id<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        kind: ThirdPartyItemKind,
        source_id: &str,
    ) -> Result<bool, UniversalInboxError>;

    async fn find_third_party_items_for_source_id<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        kind: ThirdPartyItemKind,
        source_id: &str,
    ) -> Result<Vec<ThirdPartyItem>, UniversalInboxError>;
}

#[async_trait]
impl ThirdPartyItemRepository for Repository {
    #[tracing::instrument(
        level = "debug",
        skip(self, executor, third_party_item),
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            source_id = third_party_item.source_id.as_str(),
            kind = third_party_item.kind().to_string(),
            user_id = third_party_item.user_id.to_string(),
            integration_connection_id = third_party_item.integration_connection_id.to_string()
        )
    )]
    async fn create_or_update_third_party_item<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        third_party_item: Box<ThirdPartyItem>,
    ) -> Result<UpsertStatus<Box<ThirdPartyItem>>, UniversalInboxError> {
        let data = Json(third_party_item.data.clone());
        let kind = third_party_item.kind();

        let existing_third_party_item: Option<ThirdPartyItem> = sqlx::query_as!(
            ThirdPartyItemRow,
            r#"
              SELECT
                id,
                source_id,
                data as "data: Json<ThirdPartyItemData>",
                created_at,
                updated_at,
                user_id,
                integration_connection_id
              FROM third_party_item
              WHERE
                source_id = $1
                AND kind::TEXT = $2
                AND user_id = $3
                AND integration_connection_id = $4
            "#,
            third_party_item.source_id,
            kind.to_string(),
            third_party_item.user_id.0,
            third_party_item.integration_connection_id.0
        )
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!(
                "Failed to search for third_party_item with source ID {} from storage: {err}",
                third_party_item.source_id
            );
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?
        .map(TryInto::try_into)
        .transpose()?;

        if let Some(existing_third_party_item) = existing_third_party_item {
            if existing_third_party_item == *third_party_item {
                debug!(
                    "Existing third_party_item {} {} (from {}) for {} does not need updating",
                    kind,
                    existing_third_party_item.id,
                    third_party_item.source_id,
                    third_party_item.user_id
                );
                return Ok(UpsertStatus::Untouched(Box::new(existing_third_party_item)));
            }

            debug!(
                "Updating existing third_party_item {} {} (from {}) for {}",
                kind,
                existing_third_party_item.id,
                third_party_item.source_id,
                third_party_item.user_id
            );
            let mut query_builder = QueryBuilder::new("UPDATE third_party_item SET ");
            let mut separated = query_builder.separated(", ");
            separated
                .push("data = ")
                .push_bind_unseparated(data.clone());
            separated
                .push("updated_at = ")
                .push_bind_unseparated(third_party_item.updated_at.naive_utc());
            query_builder
                .push(" WHERE id = ")
                .push_bind(existing_third_party_item.id.0);

            query_builder
                .build()
                .execute(&mut **executor)
                .await
                .map_err(|err| {
                    let message = format!(
                        "Failed to update third_party_item {} from storage: {err}",
                        existing_third_party_item.id
                    );
                    UniversalInboxError::DatabaseError {
                        source: err,
                        message,
                    }
                })?;

            let third_party_item_to_return = Box::new(ThirdPartyItem {
                data: third_party_item.data.clone(),
                updated_at: third_party_item.updated_at,
                ..existing_third_party_item.clone()
            });
            return Ok(UpsertStatus::Updated {
                new: third_party_item_to_return,
                old: Box::new(existing_third_party_item),
            });
        }

        debug!(
            "Creating new {} third_party_item {} (from {}) for {}",
            kind, third_party_item.id, third_party_item.source_id, third_party_item.user_id
        );
        let query = sqlx::query_scalar!(
            r#"
                INSERT INTO third_party_item
                  (
                    id,
                    source_id,
                    data,
                    created_at,
                    updated_at,
                    user_id,
                    integration_connection_id
                  )
                VALUES
                  ($1, $2, $3, $4, $5, $6, $7)
                RETURNING
                  id
                "#,
            third_party_item.id.0, // no need to return the id as we already know it
            third_party_item.source_id,
            data as Json<ThirdPartyItemData>, // force the macro to ignore type checking
            third_party_item.created_at.naive_utc(),
            third_party_item.updated_at.naive_utc(),
            third_party_item.user_id.0,
            third_party_item.integration_connection_id.0
        );

        let third_party_item_id = query
            .fetch_one(&mut **executor)
            .await
            .map_err(|err| {
                UniversalInboxError::Unexpected(anyhow!(
                    "Failed to update third_party_item with source ID {} from storage: {err}",
                    third_party_item.source_id
                ))
            })?
            .into();
        Ok(UpsertStatus::Created(Box::new(ThirdPartyItem {
            id: third_party_item_id,
            ..*third_party_item
        })))
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    async fn get_stale_task_source_third_party_items<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        active_task_source_third_party_item_ids: Vec<ThirdPartyItemId>,
        task_source_kind: TaskSourceKind,
        user_id: UserId,
    ) -> Result<Vec<ThirdPartyItem>, UniversalInboxError> {
        let third_party_item_ids_to_exclude = active_task_source_third_party_item_ids
            .iter()
            .map(|id| id.0)
            .collect::<Vec<Uuid>>();

        let records = sqlx::query_as!(
            ThirdPartyItemRow,
            r#"
              SELECT
                third_party_item.id,
                third_party_item.source_id,
                third_party_item.data as "data: Json<ThirdPartyItemData>",
                third_party_item.created_at,
                third_party_item.updated_at,
                third_party_item.user_id,
                third_party_item.integration_connection_id
              FROM third_party_item
              LEFT JOIN task ON task.source_item_id = third_party_item.id
              WHERE
                NOT third_party_item.id = ANY($1)
                AND task.kind::TEXT = $2
                AND third_party_item.user_id = $3
                AND task.status::TEXT = 'Active'
            "#,
            &third_party_item_ids_to_exclude[..],
            task_source_kind.to_string(),
            user_id.0,
        )
        .fetch_all(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!("Failed to get stale third party items from storage: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        records
            .iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<ThirdPartyItem>, UniversalInboxError>>()
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    async fn has_third_party_item_for_source_id<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        kind: ThirdPartyItemKind,
        source_id: &str,
    ) -> Result<bool, UniversalInboxError> {
        let count: Option<i64> = sqlx::query_scalar!(
            r#"
              SELECT
                count(*)
              FROM third_party_item
              WHERE
                source_id = $1
                AND kind::TEXT = $2
            "#,
            source_id,
            kind.to_string(),
        )
        .fetch_one(&mut **executor)
        .await
        .map_err(|err| {
            let message =
                format!("Failed to find {kind} third party item from source_id {source_id} from storage: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        if let Some(1) = count {
            return Ok(true);
        }
        return Ok(false);
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    async fn find_third_party_items_for_source_id<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        kind: ThirdPartyItemKind,
        source_id: &str,
    ) -> Result<Vec<ThirdPartyItem>, UniversalInboxError> {
        let records = sqlx::query_as!(
            ThirdPartyItemRow,
            r#"
              SELECT
                third_party_item.id,
                third_party_item.source_id,
                third_party_item.data as "data: Json<ThirdPartyItemData>",
                third_party_item.created_at,
                third_party_item.updated_at,
                third_party_item.user_id,
                third_party_item.integration_connection_id
              FROM third_party_item
              WHERE
                source_id = $1
                AND kind::TEXT = $2
            "#,
            source_id,
            kind.to_string(),
        )
        .fetch_all(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!("Failed to find {kind} third party item from source_id {source_id} from storage: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        records
            .iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<ThirdPartyItem>, UniversalInboxError>>()
    }
}

#[derive(Debug, sqlx::FromRow, Clone)]
pub struct ThirdPartyItemRow {
    pub id: Uuid,
    pub source_id: String,
    pub data: Json<ThirdPartyItemData>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub user_id: Uuid,
    pub integration_connection_id: Uuid,
}

impl TryFrom<ThirdPartyItemRow> for ThirdPartyItem {
    type Error = UniversalInboxError;

    fn try_from(row: ThirdPartyItemRow) -> Result<Self, Self::Error> {
        (&row).try_into()
    }
}

impl FromRowWithPrefix<'_, PgRow> for ThirdPartyItemRow {
    fn from_row_with_prefix(row: &PgRow, prefix: &str) -> sqlx::Result<Self> {
        Ok(ThirdPartyItemRow {
            id: row.try_get(format!("{prefix}id").as_str())?,
            source_id: row.try_get(format!("{prefix}source_id").as_str())?,
            data: row.try_get(format!("{prefix}data").as_str())?,
            created_at: row.try_get(format!("{prefix}created_at").as_str())?,
            updated_at: row.try_get(format!("{prefix}updated_at").as_str())?,
            user_id: row.try_get(format!("{prefix}user_id").as_str())?,
            integration_connection_id: row
                .try_get(format!("{prefix}integration_connection_id").as_str())?,
        })
    }
}

impl TryFrom<&ThirdPartyItemRow> for ThirdPartyItem {
    type Error = UniversalInboxError;

    fn try_from(row: &ThirdPartyItemRow) -> Result<Self, Self::Error> {
        Ok(ThirdPartyItem {
            id: row.id.into(),
            source_id: row.source_id.clone(),
            data: row.data.0.clone(),
            created_at: DateTime::from_naive_utc_and_offset(row.created_at, Utc),
            updated_at: DateTime::from_naive_utc_and_offset(row.updated_at, Utc),
            user_id: row.user_id.into(),
            integration_connection_id: row.integration_connection_id.into(),
        })
    }
}
