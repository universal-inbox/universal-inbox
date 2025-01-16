use anyhow::anyhow;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::{postgres::PgRow, types::Json, FromRow, Postgres, QueryBuilder, Row, Transaction};
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

    async fn find_third_party_items_for_user_id<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        kind: ThirdPartyItemKind,
        user_id: UserId,
    ) -> Result<Vec<ThirdPartyItem>, UniversalInboxError>;
}

#[async_trait]
impl ThirdPartyItemRepository for Repository {
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            source_id = third_party_item.source_id.as_str(),
            kind = third_party_item.kind().to_string(),
            user.id = third_party_item.user_id.to_string(),
            integration_connection_id = third_party_item.integration_connection_id.to_string()
        ),
        err
    )]
    async fn create_or_update_third_party_item<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        third_party_item: Box<ThirdPartyItem>,
    ) -> Result<UpsertStatus<Box<ThirdPartyItem>>, UniversalInboxError> {
        let data = Json(third_party_item.data.clone());
        let kind = third_party_item.kind();

        let mut query_builder = QueryBuilder::new(
            r#"
              SELECT
                third_party_item.id as third_party_item__id,
                third_party_item.source_id as third_party_item__source_id,
                third_party_item.data as third_party_item__data,
                third_party_item.created_at as third_party_item__created_at,
                third_party_item.updated_at as third_party_item__updated_at,
                third_party_item.user_id as third_party_item__user_id,
                third_party_item.integration_connection_id as third_party_item__integration_connection_id,
                source_item.id as third_party_item__si__id,
                source_item.source_id as third_party_item__si__source_id,
                source_item.data as third_party_item__si__data,
                source_item.created_at as third_party_item__si__created_at,
                source_item.updated_at as third_party_item__si__updated_at,
                source_item.user_id as third_party_item__si__user_id,
                source_item.integration_connection_id as third_party_item__si__integration_connection_id
              FROM third_party_item
              LEFT JOIN third_party_item as source_item ON third_party_item.source_item_id = source_item.id
              WHERE
            "#,
        );
        let mut separated = query_builder.separated(" AND ");
        separated
            .push("third_party_item.source_id = ")
            .push_bind_unseparated(&third_party_item.source_id)
            .push("third_party_item.kind::TEXT = ")
            .push_bind_unseparated(kind.to_string())
            .push("third_party_item.user_id = ")
            .push_bind_unseparated(third_party_item.user_id.0)
            .push("third_party_item.integration_connection_id = ")
            .push_bind_unseparated(third_party_item.integration_connection_id.0);

        let existing_third_party_item: Option<ThirdPartyItem> = query_builder
            .build_query_as::<ThirdPartyItemRow>()
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
                    integration_connection_id,
                    source_item_id
                  )
                VALUES
                  ($1, $2, $3, $4, $5, $6, $7, $8)
                RETURNING
                  id
                "#,
            third_party_item.id.0, // no need to return the id as we already know it
            third_party_item.source_id,
            data as Json<ThirdPartyItemData>, // force the macro to ignore type checking
            third_party_item.created_at.naive_utc(),
            third_party_item.updated_at.naive_utc(),
            third_party_item.user_id.0,
            third_party_item.integration_connection_id.0,
            third_party_item.source_item.as_ref().map(|item| item.id.0)
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

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            task_source_kind = task_source_kind.to_string(),
            user.id = user_id.to_string()
        ),
        err
    )]
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

        let mut query_builder = QueryBuilder::new(
            r#"
              SELECT
                third_party_item.id as third_party_item__id,
                third_party_item.source_id as third_party_item__source_id,
                third_party_item.data as third_party_item__data,
                third_party_item.created_at as third_party_item__created_at,
                third_party_item.updated_at as third_party_item__updated_at,
                third_party_item.user_id as third_party_item__user_id,
                third_party_item.integration_connection_id as third_party_item__integration_connection_id,
                source_item.id as third_party_item__si__id,
                source_item.source_id as third_party_item__si__source_id,
                source_item.data as third_party_item__si__data,
                source_item.created_at as third_party_item__si__created_at,
                source_item.updated_at as third_party_item__si__updated_at,
                source_item.user_id as third_party_item__si__user_id,
                source_item.integration_connection_id as third_party_item__si__integration_connection_id
              FROM third_party_item
              LEFT JOIN task ON task.source_item_id = third_party_item.id
              LEFT JOIN third_party_item as source_item ON third_party_item.source_item_id = source_item.id
              WHERE
            "#,
        );

        let mut separated = query_builder.separated(" AND ");
        separated
            .push("NOT third_party_item.id = ANY(")
            .push_bind_unseparated(&third_party_item_ids_to_exclude[..])
            .push_unseparated(")");
        separated
            .push("task.kind::TEXT = ")
            .push_bind_unseparated(task_source_kind.to_string());
        separated.push("task.status = 'Active'");
        separated
            .push("third_party_item.user_id = ")
            .push_bind_unseparated(user_id.0);

        let rows = query_builder
            .build_query_as::<ThirdPartyItemRow>()
            .fetch_all(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to get stale third party items from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        rows.iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<ThirdPartyItem>, UniversalInboxError>>()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            kind = kind.to_string(),
            source_id = source_id,
        ),
        err
    )]
    async fn has_third_party_item_for_source_id<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        kind: ThirdPartyItemKind,
        source_id: &str,
    ) -> Result<bool, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new("SELECT count(*) FROM third_party_item");
        query_builder.push(" WHERE source_id = ");
        query_builder.push_bind(source_id);
        query_builder.push(" AND kind::TEXT = ");
        query_builder.push_bind(kind.to_string());

        let count: Option<i64> = query_builder
            .build_query_scalar()
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

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            kind = kind.to_string(),
            source_id = source_id,
        ),
        err
    )]
    async fn find_third_party_items_for_source_id<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        kind: ThirdPartyItemKind,
        source_id: &str,
    ) -> Result<Vec<ThirdPartyItem>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
              SELECT
                third_party_item.id as third_party_item__id,
                third_party_item.source_id as third_party_item__source_id,
                third_party_item.data as third_party_item__data,
                third_party_item.created_at as third_party_item__created_at,
                third_party_item.updated_at as third_party_item__updated_at,
                third_party_item.user_id as third_party_item__user_id,
                third_party_item.integration_connection_id as third_party_item__integration_connection_id,
                source_item.id as third_party_item__si__id,
                source_item.source_id as third_party_item__si__source_id,
                source_item.data as third_party_item__si__data,
                source_item.created_at as third_party_item__si__created_at,
                source_item.updated_at as third_party_item__si__updated_at,
                source_item.user_id as third_party_item__si__user_id,
                source_item.integration_connection_id as third_party_item__si__integration_connection_id
              FROM third_party_item
              LEFT JOIN third_party_item as source_item ON third_party_item.source_item_id = source_item.id
            "#,
        );
        query_builder.push(" WHERE third_party_item.source_id = ");
        query_builder.push_bind(source_id);
        query_builder.push(" AND third_party_item.kind::TEXT = ");
        query_builder.push_bind(kind.to_string());

        let records = query_builder
            .build_query_as::<ThirdPartyItemRow>()
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

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            kind = kind.to_string(),
            user_id = user_id.to_string(),
        ),
        err
    )]
    async fn find_third_party_items_for_user_id<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        kind: ThirdPartyItemKind,
        user_id: UserId,
    ) -> Result<Vec<ThirdPartyItem>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
              SELECT
                third_party_item.id as third_party_item__id,
                third_party_item.source_id as third_party_item__source_id,
                third_party_item.data as third_party_item__data,
                third_party_item.created_at as third_party_item__created_at,
                third_party_item.updated_at as third_party_item__updated_at,
                third_party_item.user_id as third_party_item__user_id,
                third_party_item.integration_connection_id as third_party_item__integration_connection_id,
                source_item.id as third_party_item__si__id,
                source_item.source_id as third_party_item__si__source_id,
                source_item.data as third_party_item__si__data,
                source_item.created_at as third_party_item__si__created_at,
                source_item.updated_at as third_party_item__si__updated_at,
                source_item.user_id as third_party_item__si__user_id,
                source_item.integration_connection_id as third_party_item__si__integration_connection_id
              FROM third_party_item
              LEFT JOIN third_party_item as source_item ON third_party_item.source_item_id = source_item.id
            "#,
        );
        query_builder.push(" WHERE third_party_item.user_id = ");
        query_builder.push_bind(user_id.0);
        query_builder.push(" AND third_party_item.kind::TEXT = ");
        query_builder.push_bind(kind.to_string());

        let records = query_builder
            .build_query_as::<ThirdPartyItemRow>()
            .fetch_all(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to find {kind} third party item for user_id {user_id} from storage: {err}");
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

#[derive(Debug, Clone)]
pub struct ThirdPartyItemRow {
    pub id: Uuid,
    pub source_id: String,
    pub data: Json<ThirdPartyItemData>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub user_id: Uuid,
    pub integration_connection_id: Uuid,
    pub source_item: Option<Box<ThirdPartyItemRow>>,
}

impl TryFrom<ThirdPartyItemRow> for ThirdPartyItem {
    type Error = UniversalInboxError;

    fn try_from(row: ThirdPartyItemRow) -> Result<Self, Self::Error> {
        (&row).try_into()
    }
}

impl FromRow<'_, PgRow> for ThirdPartyItemRow {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        ThirdPartyItemRow::from_row_with_prefix(row, "third_party_item__")
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
            source_item: row
                .try_get::<Option<Uuid>, &str>(format!("{prefix}si__id").as_str())
                .or_else(|err| match err {
                    // Stop the recursion if the column is not found
                    sqlx::Error::ColumnNotFound(_) => Ok(None),
                    _ => Err(err),
                })?
                .map(|_source_item_id| {
                    ThirdPartyItemRow::from_row_with_prefix(row, format!("{prefix}si__").as_str())
                })
                .transpose()?
                .map(Box::new),
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
            source_item: row
                .source_item
                .as_ref()
                .map(|r| (&**r).try_into())
                .transpose()?
                .map(Box::new),
        })
    }
}
