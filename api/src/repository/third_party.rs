use anyhow::anyhow;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::{
    FromRow, PgConnection, Postgres, QueryBuilder, Row, Transaction, postgres::PgRow, types::Json,
};
use tracing::{debug, warn};
use uuid::Uuid;

use universal_inbox::{
    notification::NotificationStatus,
    task::TaskSourceKind,
    third_party::item::{ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemId, ThirdPartyItemKind},
    user::UserId,
};

use crate::observability::attr;
use crate::{
    repository::Repository,
    universal_inbox::{UniversalInboxError, UpsertStatus},
};

use super::{FromRowWithPrefix, decode_rows_skipping_invalid};

#[async_trait]
pub trait ThirdPartyItemRepository {
    async fn create_or_update_third_party_item(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        third_party_item: Box<ThirdPartyItem>,
    ) -> Result<UpsertStatus<Box<ThirdPartyItem>>, UniversalInboxError>;

    async fn get_stale_task_source_third_party_items(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        active_task_source_third_party_item_ids: Vec<ThirdPartyItemId>,
        task_source_kind: TaskSourceKind,
        user_id: UserId,
    ) -> Result<Vec<ThirdPartyItem>, UniversalInboxError>;

    async fn has_third_party_item_for_source_id(
        &self,
        executor: &mut PgConnection,
        kind: ThirdPartyItemKind,
        source_id: &str,
    ) -> Result<bool, UniversalInboxError>;

    /// Whether the third-party item `id` exists and belongs to `user_id`.
    async fn is_third_party_item_owned_by(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: ThirdPartyItemId,
        user_id: UserId,
    ) -> Result<bool, UniversalInboxError>;

    async fn find_third_party_items_for_source_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        kind: ThirdPartyItemKind,
        source_id: &str,
        user_id: Option<UserId>,
    ) -> Result<Vec<ThirdPartyItem>, UniversalInboxError>;

    async fn find_third_party_items_for_user_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        kind: ThirdPartyItemKind,
        user_id: UserId,
    ) -> Result<Vec<ThirdPartyItem>, UniversalInboxError>;

    async fn find_third_party_items_with_active_notification_for_user_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        kind: ThirdPartyItemKind,
        notification_status: NotificationStatus,
        user_id: UserId,
    ) -> Result<Vec<ThirdPartyItem>, UniversalInboxError>;

    /// Todoist items still stored with a legacy (pre API v1) all-digit ID,
    /// excluding those whose task is already `Deleted`.
    async fn find_legacy_todoist_items(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: Option<UserId>,
    ) -> Result<Vec<ThirdPartyItem>, UniversalInboxError>;

    async fn update_third_party_item_source_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: ThirdPartyItemId,
        source_id: &str,
        data: &ThirdPartyItemData,
    ) -> Result<(), UniversalInboxError>;
}

#[async_trait]
impl ThirdPartyItemRepository for Repository {
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::THIRD_PARTY_ITEM_ID } = third_party_item.id.to_string(),
            { attr::THIRD_PARTY_ITEM_SOURCE_ID } = third_party_item.source_id.as_str(),
            { attr::THIRD_PARTY_ITEM_KIND } = third_party_item.kind().to_string(),
            { attr::USER_ID } = third_party_item.user_id.to_string(),
            { attr::INTEGRATION_CONNECTION_ID } = third_party_item.integration_connection_id.to_string()
        )
    )]
    async fn create_or_update_third_party_item(
        &self,
        executor: &mut Transaction<'_, Postgres>,
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

        let existing_row = query_builder
            .build()
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
            })?;

        let (existing_third_party_item, is_healing) = match existing_row {
            None => (None, false),
            Some(row) => match ThirdPartyItemRow::from_row(&row) {
                Ok(existing_row) => (Some(ThirdPartyItem::try_from(existing_row)?), false),
                Err(decode_err) => {
                    // The stored data no longer matches the current types (eg. after a third
                    // party payload shape change): only decode the plain columns and let the
                    // update below overwrite the data with the fresh upstream one.
                    let decode_plain_columns = || -> sqlx::Result<(Uuid, NaiveDateTime)> {
                        Ok((
                            row.try_get("third_party_item__id")?,
                            row.try_get("third_party_item__created_at")?,
                        ))
                    };
                    let (id, created_at) = decode_plain_columns().map_err(|err| {
                        let message = format!(
                            "Failed to decode third_party_item with source ID {} from storage: {err}",
                            third_party_item.source_id
                        );
                        UniversalInboxError::DatabaseError {
                            source: err,
                            message,
                        }
                    })?;
                    warn!(
                        "Healing {} third_party_item {} (from {}) for {} that cannot be decoded: {decode_err}",
                        kind, id, third_party_item.source_id, third_party_item.user_id
                    );
                    // The undecodable data cannot be returned as the `old` value, use the new
                    // one instead
                    let healed_third_party_item = ThirdPartyItem {
                        id: id.into(),
                        created_at: DateTime::from_naive_utc_and_offset(created_at, Utc),
                        ..*third_party_item.clone()
                    };
                    (Some(healed_third_party_item), true)
                }
            },
        };

        if let Some(existing_third_party_item) = existing_third_party_item {
            if !is_healing && existing_third_party_item == *third_party_item {
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
            separated
                .push("source_item_id = ")
                .push_bind_unseparated(third_party_item.source_item.as_ref().map(|item| item.id.0));
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
            { attr::SYNC_SOURCE_KIND } = task_source_kind.to_string(),
            { attr::USER_ID } = user_id.to_string()
        )
    )]
    async fn get_stale_task_source_third_party_items(
        &self,
        executor: &mut Transaction<'_, Postgres>,
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
            .build()
            .fetch_all(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to get stale third party items from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        decode_rows_skipping_invalid::<ThirdPartyItemRow>(
            &rows,
            "third_party_item__id",
            "third party item",
        )
        .iter()
        .map(|r| r.try_into())
        .collect::<Result<Vec<ThirdPartyItem>, UniversalInboxError>>()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::THIRD_PARTY_ITEM_ID } = id.to_string(), { attr::USER_ID } = user_id.to_string())
    )]
    async fn is_third_party_item_owned_by(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: ThirdPartyItemId,
        user_id: UserId,
    ) -> Result<bool, UniversalInboxError> {
        let owned = sqlx::query_scalar!(
            r#"
                SELECT EXISTS(
                  SELECT 1 FROM third_party_item WHERE id = $1 AND user_id = $2
                ) AS "owned!"
            "#,
            id.0,
            user_id.0
        )
        .fetch_one(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!("Failed to check owner of third party item {id}: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;
        Ok(owned)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::THIRD_PARTY_ITEM_KIND } = kind.to_string(),
            { attr::THIRD_PARTY_ITEM_SOURCE_ID } = source_id,
        )
    )]
    async fn has_third_party_item_for_source_id(
        &self,
        executor: &mut PgConnection,
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
            .fetch_one(&mut *executor)
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
            { attr::THIRD_PARTY_ITEM_KIND } = kind.to_string(),
            { attr::THIRD_PARTY_ITEM_SOURCE_ID } = source_id,
            { attr::USER_ID } = user_id.map(|id| id.to_string()),
        )
    )]
    async fn find_third_party_items_for_source_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        kind: ThirdPartyItemKind,
        source_id: &str,
        user_id: Option<UserId>,
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
        if let Some(user_id) = user_id {
            query_builder.push(" AND third_party_item.user_id = ");
            query_builder.push_bind(user_id.0);
        }

        let records = query_builder
            .build()
            .fetch_all(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to find {kind} third party item from source_id {source_id} from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        decode_rows_skipping_invalid::<ThirdPartyItemRow>(
            &records,
            "third_party_item__id",
            "third party item",
        )
        .iter()
        .map(|r| r.try_into())
        .collect::<Result<Vec<ThirdPartyItem>, UniversalInboxError>>()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::THIRD_PARTY_ITEM_KIND } = kind.to_string(),
            { attr::USER_ID } = user_id.to_string(),
        )
    )]
    async fn find_third_party_items_for_user_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
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
            .build()
            .fetch_all(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to find {kind} third party item for user_id {user_id} from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        decode_rows_skipping_invalid::<ThirdPartyItemRow>(
            &records,
            "third_party_item__id",
            "third party item",
        )
        .iter()
        .map(|r| r.try_into())
        .collect::<Result<Vec<ThirdPartyItem>, UniversalInboxError>>()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::THIRD_PARTY_ITEM_KIND } = kind.to_string(), { attr::NOTIFICATION_STATUS } = notification_status.to_string(), { attr::USER_ID } = user_id.to_string())
    )]
    async fn find_third_party_items_with_active_notification_for_user_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        kind: ThirdPartyItemKind,
        notification_status: NotificationStatus,
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
              INNER JOIN notification ON notification.source_item_id = third_party_item.id
            "#,
        );
        query_builder.push(" WHERE third_party_item.user_id = ");
        query_builder.push_bind(user_id.0);
        query_builder.push(" AND third_party_item.kind::TEXT = ");
        query_builder.push_bind(kind.to_string());
        query_builder.push(" AND notification.status::TEXT = ");
        query_builder.push_bind(notification_status.to_string());

        let records = query_builder
            .build()
            .fetch_all(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to find {kind} third party items with {notification_status} notification for user_id {user_id} from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        decode_rows_skipping_invalid::<ThirdPartyItemRow>(
            &records,
            "third_party_item__id",
            "third party item",
        )
        .iter()
        .map(|r| r.try_into())
        .collect::<Result<Vec<ThirdPartyItem>, UniversalInboxError>>()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.map(|id| id.to_string()))
    )]
    async fn find_legacy_todoist_items(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: Option<UserId>,
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
              LEFT JOIN task ON task.source_item_id = third_party_item.id
              LEFT JOIN third_party_item as source_item ON third_party_item.source_item_id = source_item.id
              WHERE third_party_item.kind::TEXT = 'TodoistItem'
                AND third_party_item.source_id ~ '^[0-9]+$'
                AND (task.id IS NULL OR task.status != 'Deleted')
            "#,
        );
        if let Some(user_id) = user_id {
            query_builder.push(" AND third_party_item.user_id = ");
            query_builder.push_bind(user_id.0);
        }

        let records = query_builder
            .build()
            .fetch_all(&mut **executor)
            .await
            .map_err(|err| {
                let message =
                    format!("Failed to find legacy Todoist third party items from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        decode_rows_skipping_invalid::<ThirdPartyItemRow>(
            &records,
            "third_party_item__id",
            "third party item",
        )
        .iter()
        .map(|r| r.try_into())
        .collect::<Result<Vec<ThirdPartyItem>, UniversalInboxError>>()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::THIRD_PARTY_ITEM_ID } = id.to_string(), { attr::THIRD_PARTY_ITEM_SOURCE_ID } = %source_id)
    )]
    async fn update_third_party_item_source_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: ThirdPartyItemId,
        source_id: &str,
        data: &ThirdPartyItemData,
    ) -> Result<(), UniversalInboxError> {
        sqlx::query(
            r#"
              UPDATE third_party_item
              SET source_id = $1, data = $2, updated_at = $3
              WHERE id = $4
            "#,
        )
        .bind(source_id)
        .bind(Json(data))
        .bind(Utc::now().naive_utc())
        .bind(id.0)
        .execute(&mut **executor)
        .await
        .map_err(|err| {
            let message =
                format!("Failed to update source_id of third party item {id} in storage: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        Ok(())
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
