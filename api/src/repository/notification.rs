use anyhow::{anyhow, Context};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::{postgres::PgRow, types::Json, FromRow, Postgres, QueryBuilder, Row, Transaction};
use url::Url;
use uuid::Uuid;

use universal_inbox::{
    notification::{
        service::NotificationPatch, Notification, NotificationDetails, NotificationId,
        NotificationMetadata, NotificationSourceKind, NotificationStatus, NotificationWithTask,
    },
    task::TaskId,
    user::UserId,
};

use crate::{
    repository::{task::TaskRow, Repository},
    universal_inbox::{UniversalInboxError, UpdateStatus, UpsertStatus},
};

#[async_trait]
pub trait NotificationRepository {
    async fn get_one_notification<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        id: NotificationId,
    ) -> Result<Option<Notification>, UniversalInboxError>;
    async fn get_notification_for_source_id<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        source_id: &str,
        user_id: UserId,
    ) -> Result<Option<Notification>, UniversalInboxError>;
    async fn does_notification_exist<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        id: NotificationId,
    ) -> Result<bool, UniversalInboxError>;
    async fn fetch_all_notifications<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        status: Vec<NotificationStatus>,
        include_snoozed_notifications: bool,
        task_id: Option<TaskId>,
        user_id: UserId,
    ) -> Result<Vec<NotificationWithTask>, UniversalInboxError>;
    async fn create_notification<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification: Box<Notification>,
    ) -> Result<Box<Notification>, UniversalInboxError>;
    async fn update_stale_notifications_status_from_source_ids<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        active_source_notification_ids: Vec<String>,
        kind: NotificationSourceKind,
        status: NotificationStatus,
        user_id: UserId,
    ) -> Result<Vec<Notification>, UniversalInboxError>;
    async fn create_or_update_notification<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification: Box<Notification>,
        kind: NotificationSourceKind,
        update_snoozed_until: bool,
    ) -> Result<UpsertStatus<Box<Notification>>, UniversalInboxError>;
    async fn create_or_update_notification_details<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification_id: NotificationId,
        details: NotificationDetails,
    ) -> Result<UpsertStatus<NotificationDetails>, UniversalInboxError>;
    async fn update_notification<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification_id: NotificationId,
        patch: &NotificationPatch,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<Notification>>, UniversalInboxError>;
    async fn update_notifications_for_task<'a, 'b>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task_id: TaskId,
        notification_kind: Option<NotificationSourceKind>,
        patch: &'b NotificationPatch,
    ) -> Result<Vec<UpdateStatus<Notification>>, UniversalInboxError>;
    async fn delete_notification_details<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        kind: NotificationSourceKind,
    ) -> Result<u64, UniversalInboxError>;
    async fn delete_notifications<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        kind: NotificationSourceKind,
    ) -> Result<u64, UniversalInboxError>;
}

#[async_trait]
impl NotificationRepository for Repository {
    #[tracing::instrument(level = "debug", skip(self, executor))]
    async fn get_one_notification<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        id: NotificationId,
    ) -> Result<Option<Notification>, UniversalInboxError> {
        let row = sqlx::query_as!(
            NotificationRow,
            r#"
                SELECT
                  notification.id,
                  notification.title,
                  notification.status as "status: _",
                  notification.source_id,
                  notification.source_html_url,
                  notification.metadata as "metadata: Json<NotificationMetadata>",
                  notification.updated_at,
                  notification.last_read_at,
                  notification.snoozed_until,
                  notification_details.details as "details: Option<Json<NotificationDetails>>",
                  notification.task_id,
                  notification.user_id
                FROM notification
                LEFT JOIN notification_details ON notification_details.notification_id = notification.id
                WHERE notification.id = $1
            "#,
            id.0
        )
        .fetch_optional(&mut **executor)
        .await
        .with_context(|| format!("Failed to fetch notification {id} from storage"))?;

        row.map(|notification_row| notification_row.try_into())
            .transpose()
    }

    async fn get_notification_for_source_id<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        source_id: &str,
        user_id: UserId,
    ) -> Result<Option<Notification>, UniversalInboxError> {
        let row = sqlx::query_as!(
            NotificationRow,
            r#"
                SELECT
                  notification.id,
                  notification.title,
                  notification.status as "status: _",
                  notification.source_id,
                  notification.source_html_url,
                  notification.metadata as "metadata: Json<NotificationMetadata>",
                  notification.updated_at,
                  notification.last_read_at,
                  notification.snoozed_until,
                  notification_details.details as "details: Option<Json<NotificationDetails>>",
                  notification.task_id,
                  notification.user_id
                FROM notification
                LEFT JOIN notification_details ON notification_details.notification_id = notification.id
                WHERE notification.source_id = $1
                AND notification.user_id = $2
            "#,
            source_id,
            user_id.0
        )
            .fetch_optional(&mut **executor)
            .await
            .with_context(|| format!("Failed to fetch notification with source ID {source_id} and user ID {user_id} from storage"))?;

        row.map(|notification_row| notification_row.try_into())
            .transpose()
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    async fn does_notification_exist<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        id: NotificationId,
    ) -> Result<bool, UniversalInboxError> {
        let count: Option<i64> =
            sqlx::query_scalar!(r#"SELECT count(*) FROM notification WHERE id = $1"#, id.0)
                .fetch_one(&mut **executor)
                .await
                .with_context(|| format!("Failed to check if notification {id} exists",))?;

        if let Some(1) = count {
            return Ok(true);
        }
        return Ok(false);
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    async fn fetch_all_notifications<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        status: Vec<NotificationStatus>,
        include_snoozed_notifications: bool,
        task_id: Option<TaskId>,
        user_id: UserId,
    ) -> Result<Vec<NotificationWithTask>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
                SELECT
                  notification.id as notification_id,
                  notification.title as notification_title,
                  notification.status as notification_status,
                  notification.source_id as notification_source_id,
                  notification.source_html_url as notification_source_html_url,
                  notification.metadata as notification_metadata,
                  notification.updated_at as notification_updated_at,
                  notification.last_read_at as notification_last_read_at,
                  notification.snoozed_until as notification_snoozed_until,
                  notification.user_id as notification_user_id,
                  notification_details.details as notification_details,
                  task.id,
                  task.source_id,
                  task.title,
                  task.body,
                  task.status,
                  task.completed_at,
                  task.priority,
                  task.due_at,
                  task.source_html_url,
                  task.tags,
                  task.parent_id,
                  task.project,
                  task.is_recurring,
                  task.created_at,
                  task.metadata,
                  task.user_id
                FROM
                  notification
                LEFT JOIN notification_details ON notification_details.notification_id = notification.id
                LEFT JOIN task ON task.id = notification.task_id
                WHERE
            "#,
        );

        let mut separated = query_builder.separated(" AND ");
        if !status.is_empty() {
            separated
                .push("notification.status::TEXT = ANY(")
                .push_bind_unseparated(
                    status
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<String>>(),
                )
                .push_unseparated(")");
        }
        separated
            .push(" notification.user_id = ")
            .push_bind_unseparated(user_id.0);

        if !include_snoozed_notifications {
            separated
                .push(" (notification.snoozed_until is NULL OR notification.snoozed_until <=")
                .push_bind_unseparated(Utc::now().naive_utc())
                .push_unseparated(")");
        }

        if let Some(id) = task_id {
            separated
                .push(" notification.task_id = ")
                .push_bind_unseparated(id.0);
        }

        query_builder.push(" ORDER BY notification.updated_at ASC LIMIT 100");

        let records = query_builder
            .build_query_as::<NotificationWithTaskRow>()
            .fetch_all(&mut **executor)
            .await
            .context("Failed to fetch notifications from storage")?;

        records.iter().map(|r| r.try_into()).collect()
    }

    #[tracing::instrument(level = "debug", skip(self, executor, notification), fields(notification_id = notification.id.to_string()))]
    async fn create_notification<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification: Box<Notification>,
    ) -> Result<Box<Notification>, UniversalInboxError> {
        let metadata = Json(notification.metadata.clone());

        sqlx::query!(
            r#"
                INSERT INTO notification
                  (
                    id,
                    title,
                    status,
                    source_id,
                    source_html_url,
                    metadata,
                    updated_at,
                    last_read_at,
                    snoozed_until,
                    user_id,
                    task_id
                  )
                VALUES
                  ($1, $2, $3::notification_status, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
            notification.id.0,
            notification.title,
            notification.status.to_string() as _,
            notification.source_id,
            notification
                .source_html_url
                .as_ref()
                .map(|url| url.to_string()),
            metadata as Json<NotificationMetadata>, // force the macro to ignore type checking
            notification.updated_at.naive_utc(),
            notification
                .last_read_at
                .map(|last_read_at| last_read_at.naive_utc()),
            notification
                .snoozed_until
                .map(|snoozed_until| snoozed_until.naive_utc()),
            notification.user_id.0,
            notification.task_id.map(|task_id| task_id.0),
        )
        .execute(&mut **executor)
        .await
        .map_err(|e| {
            match e
                .as_database_error()
                .and_then(|db_error| db_error.code().map(|code| code.to_string()))
            {
                Some(x) if x == *"23505" => UniversalInboxError::AlreadyExists {
                    source: e,
                    id: notification.id.0,
                },
                _ => UniversalInboxError::Unexpected(anyhow!(
                    "Failed to insert new notification into storage"
                )),
            }
        })?;

        Ok(notification)
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    async fn update_stale_notifications_status_from_source_ids<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        active_source_notification_ids: Vec<String>,
        kind: NotificationSourceKind,
        status: NotificationStatus,
        user_id: UserId,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        let records = sqlx::query_as!(
            NotificationRow,
            r#"
                UPDATE
                  notification
                SET
                  status = $1::notification_status
                FROM notification as n
                LEFT JOIN notification_details ON notification_details.notification_id = n.id
                WHERE
                  NOT notification.source_id = ANY($2)
                  AND notification.kind = $3
                  AND (notification.status::TEXT = 'Read' OR notification.status::TEXT = 'Unread')
                  AND notification.user_id = $4
                RETURNING
                  notification.id,
                  notification.title,
                  notification.status as "status: _",
                  notification.source_id,
                  notification.source_html_url,
                  notification.metadata as "metadata: Json<NotificationMetadata>",
                  notification.updated_at,
                  notification.last_read_at,
                  notification.snoozed_until,
                  notification.user_id,
                  notification_details.details as "details: Option<Json<NotificationDetails>>",
                  notification.task_id
            "#,
            status.to_string() as _,
            &active_source_notification_ids[..],
            kind.to_string(),
            user_id.0,
        )
        .fetch_all(&mut **executor)
        .await
        .context("Failed to update stale notification status from storage")?;

        records
            .iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<Notification>, UniversalInboxError>>()
    }

    #[tracing::instrument(level = "debug", skip(self, executor, notification), fields(notification_id = notification.id.to_string()))]
    async fn create_or_update_notification<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification: Box<Notification>,
        kind: NotificationSourceKind,
        update_snoozed_until: bool,
    ) -> Result<UpsertStatus<Box<Notification>>, UniversalInboxError> {
        let metadata = Json(notification.metadata.clone());

        let existing_notification = sqlx::query!(
            r#"
              SELECT id, updated_at
              FROM notification
              WHERE source_id = $1 AND kind = $2
            "#,
            notification.source_id,
            kind.to_string()
        )
        .fetch_optional(&mut **executor)
        .await
        .with_context(|| {
            format!(
                "Failed to search for notification with source ID {} from storage",
                notification.source_id
            )
        })?;

        let source_html_url_str = notification
            .source_html_url
            .as_ref()
            .map(|url| url.to_string());
        let last_read_at_naive_utc = notification
            .last_read_at
            .map(|last_read_at| last_read_at.naive_utc());
        let snoozed_until_naive_utc = notification
            .snoozed_until
            .map(|snoozed_until| snoozed_until.naive_utc());

        let upsert_status = if let Some(existing_notification) = existing_notification {
            let mut query_builder = QueryBuilder::new("UPDATE notification SET ");
            let mut separated = query_builder.separated(", ");
            separated
                .push("title = ")
                .push_bind_unseparated(notification.title.clone());
            separated
                .push("status = ")
                .push_bind_unseparated(notification.status.to_string())
                .push_unseparated("::notification_status");
            separated
                .push("source_html_url = ")
                .push_bind_unseparated(source_html_url_str.clone());
            separated
                .push("metadata = ")
                .push_bind_unseparated(metadata.clone());
            separated
                .push("updated_at = ")
                .push_bind_unseparated(notification.updated_at.naive_utc());
            separated
                .push("last_read_at = ")
                .push_bind_unseparated(last_read_at_naive_utc);
            separated
                .push("user_id = ")
                .push_bind_unseparated(notification.user_id.0);
            if update_snoozed_until {
                separated
                    .push("snoozed_until = ")
                    .push_bind_unseparated(snoozed_until_naive_utc);
            }
            query_builder
                .push(" WHERE id = ")
                .push_bind(existing_notification.id);

            query_builder
                .build()
                .execute(&mut **executor)
                .await
                .context(format!(
                    "Failed to update notification {} from storage",
                    existing_notification.id
                ))?;

            let notification_to_return = Box::new(Notification {
                id: NotificationId(existing_notification.id),
                ..*notification
            });
            if existing_notification.updated_at != notification.updated_at.naive_utc() {
                UpsertStatus::Updated(notification_to_return)
            } else {
                UpsertStatus::Untouched(notification_to_return)
            }
        } else {
            let query = sqlx::query_scalar!(
                r#"
                INSERT INTO notification
                  (
                    id,
                    title,
                    status,
                    source_id,
                    source_html_url,
                    metadata,
                    updated_at,
                    last_read_at,
                    snoozed_until,
                    user_id,
                    task_id
                  )
                VALUES
                  ($1, $2, $3::notification_status, $4, $5, $6, $7, $8, $9, $10, $11)
                RETURNING
                  id
                "#,
                notification.id.0, // no need to return the id as we already know it
                notification.title,
                notification.status.to_string() as _,
                notification.source_id,
                source_html_url_str,
                metadata as Json<NotificationMetadata>, // force the macro to ignore type checking
                notification.updated_at.naive_utc(),
                last_read_at_naive_utc,
                snoozed_until_naive_utc,
                notification.user_id.0,
                notification.task_id.map(|task_id| task_id.0),
            );

            let notification_id =
                NotificationId(query.fetch_one(&mut **executor).await.with_context(|| {
                    format!(
                        "Failed to update notification with source ID {} from storage",
                        notification.source_id
                    )
                })?);
            UpsertStatus::Created(Box::new(Notification {
                id: notification_id,
                ..*notification
            }))
        };

        Ok(upsert_status)
    }

    #[tracing::instrument(level = "debug", skip(self, executor, details))]
    async fn create_or_update_notification_details<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification_id: NotificationId,
        details: NotificationDetails,
    ) -> Result<UpsertStatus<NotificationDetails>, UniversalInboxError> {
        let now = Utc::now().naive_utc();
        let new_id = Uuid::new_v4();
        let res = sqlx::query_scalar!(
            r#"
              INSERT INTO notification_details
                (
                  id,
                  created_at,
                  updated_at,
                  notification_id,
                  details
                )
              VALUES
                ($1, $2, $3, $4, $5)
              ON CONFLICT (notification_id) DO UPDATE
              SET
                updated_at = $3,
                details = $5
              RETURNING
                id
            "#,
            new_id,
            now,
            now,
            notification_id.0,
            Json(details.clone()) as Json<NotificationDetails>,
        )
        .fetch_one(&mut **executor)
        .await;

        let id = res.with_context(|| {
            format!(
                "Failed to update notification details for notification {} from storage",
                notification_id
            )
        })?;

        Ok(if id != new_id {
            UpsertStatus::Updated(details)
        } else {
            UpsertStatus::Created(details)
        })
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    async fn update_notification<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification_id: NotificationId,
        patch: &NotificationPatch,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<Notification>>, UniversalInboxError> {
        if *patch == Default::default() {
            return Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!(
                    "Missing `status` field value to update notification {notification_id}"
                ),
            });
        };

        let mut query_builder = QueryBuilder::new("UPDATE notification SET");
        let mut separated = query_builder.separated(", ");
        if let Some(status) = patch.status {
            separated
                .push(" status = ")
                .push_bind_unseparated(status.to_string())
                .push_unseparated("::notification_status");
        }
        if let Some(snoozed_until) = patch.snoozed_until {
            separated
                .push(" snoozed_until = ")
                .push_bind_unseparated(snoozed_until.naive_utc());
        }
        if let Some(task_id) = patch.task_id {
            separated
                .push(" task_id = ")
                .push_bind_unseparated(task_id.0);
        }

        query_builder
            .push(" FROM notification AS n ")
            .push(" LEFT JOIN notification_details ON notification_details.notification_id = n.id ")
            .push(" WHERE ")
            .separated(" AND ")
            .push(" notification.id = ")
            .push_bind_unseparated(notification_id.0)
            .push(" n.id = ")
            .push_bind_unseparated(notification_id.0)
            .push(" notification.user_id = ")
            .push_bind_unseparated(for_user_id.0);

        query_builder.push(
            r#"
                RETURNING
                  notification.id,
                  notification.title,
                  notification.status,
                  notification.source_id,
                  notification.source_html_url,
                  notification.metadata,
                  notification.updated_at,
                  notification.last_read_at,
                  notification.snoozed_until,
                  notification.user_id,
                  notification_details.details as details,
                  notification.task_id,
                  (SELECT"#,
        );

        let mut separated = query_builder.separated(" OR ");
        if let Some(status) = patch.status {
            separated
                .push(" status::TEXT != ")
                .push_bind_unseparated(status.to_string());
        }
        if let Some(snoozed_until) = patch.snoozed_until {
            separated
                .push(" (snoozed_until is NULL OR snoozed_until != ")
                .push_bind_unseparated(snoozed_until.naive_utc())
                .push_unseparated(")");
        }
        if let Some(task_id) = patch.task_id {
            separated
                .push(" (task_id is NULL OR task_id != ")
                .push_bind_unseparated(task_id.0)
                .push_unseparated(")");
        }

        query_builder
            .push(" FROM notification WHERE id = ")
            .push_bind(notification_id.0);
        query_builder.push(r#") as "is_updated""#);

        let record: Option<UpdatedNotificationRow> = query_builder
            .build_query_as::<UpdatedNotificationRow>()
            .fetch_optional(&mut **executor)
            .await
            .context(format!(
                "Failed to update notification {notification_id} from storage"
            ))?;

        if let Some(updated_notification_row) = record {
            Ok(UpdateStatus {
                updated: updated_notification_row.is_updated,
                result: Some(Box::new(
                    updated_notification_row
                        .notification_row
                        .try_into()
                        .unwrap(),
                )),
            })
        } else {
            Ok(UpdateStatus {
                updated: false,
                result: None,
            })
        }
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    async fn update_notifications_for_task<'a, 'b>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task_id: TaskId,
        notification_kind: Option<NotificationSourceKind>,
        patch: &'b NotificationPatch,
    ) -> Result<Vec<UpdateStatus<Notification>>, UniversalInboxError> {
        if *patch == Default::default() {
            return Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!(
                    "Missing `status` field value to update notifications for task {task_id}"
                ),
            });
        };

        let mut query_builder = QueryBuilder::new("UPDATE notification SET");
        let mut separated = query_builder.separated(", ");
        if let Some(status) = patch.status {
            separated
                .push(" status = ")
                .push_bind_unseparated(status.to_string())
                .push_unseparated("::notification_status");
        }
        if let Some(snoozed_until) = patch.snoozed_until {
            separated
                .push(" snoozed_until = ")
                .push_bind_unseparated(snoozed_until.naive_utc());
        }

        let mut separated = query_builder
            .push(" FROM notification AS n ")
            .push(" LEFT JOIN notification_details ON notification_details.notification_id = n.id ")
            .push(" WHERE ")
            .separated(" AND ");
        separated
            .push(" notification.task_id = ")
            .push_bind_unseparated(task_id.0)
            .push(" n.task_id = ")
            .push_bind_unseparated(task_id.0);

        if let Some(kind) = notification_kind {
            separated
                .push(" notification.kind = ")
                .push_bind_unseparated(kind.to_string());
        }

        query_builder.push(
            r#"
                RETURNING
                  notification.id,
                  notification.title,
                  notification.status,
                  notification.source_id,
                  notification.source_html_url,
                  notification.metadata,
                  notification.updated_at,
                  notification.last_read_at,
                  notification.snoozed_until,
                  notification.user_id,
                  notification_details.details as details,
                  notification.task_id,
                  (SELECT"#,
        );

        let mut separated = query_builder.separated(" OR ");
        if let Some(status) = patch.status {
            separated
                .push(" status::TEXT != ")
                .push_bind_unseparated(status.to_string());
        }
        if let Some(snoozed_until) = patch.snoozed_until {
            separated
                .push(" (snoozed_until is NULL OR snoozed_until != ")
                .push_bind_unseparated(snoozed_until.naive_utc())
                .push_unseparated(")");
        }

        query_builder
            .push(" FROM notification WHERE task_id = ")
            .push_bind(task_id.0);
        if let Some(kind) = notification_kind {
            query_builder
                .push(" AND kind = ")
                .push_bind(kind.to_string());
        }
        query_builder.push(r#") as "is_updated""#);

        let records: Vec<UpdatedNotificationRow> = query_builder
            .build_query_as::<UpdatedNotificationRow>()
            .fetch_all(&mut **executor)
            .await
            .context(format!(
                "Failed to update notifications for task {task_id} from storage"
            ))?;

        let update_statuses = records
            .into_iter()
            .map(|updated_notification_row| UpdateStatus {
                updated: updated_notification_row.is_updated,
                result: Some(
                    updated_notification_row
                        .notification_row
                        .try_into()
                        .unwrap(),
                ),
            })
            .collect();
        Ok(update_statuses)
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    async fn delete_notification_details<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        kind: NotificationSourceKind,
    ) -> Result<u64, UniversalInboxError> {
        let res = sqlx::query!(
            r#"
            DELETE FROM notification_details
              USING notification
            WHERE notification_details.notification_id = notification.id
              AND notification.kind = $1
            "#,
            kind.to_string()
        )
        .execute(&mut **executor)
        .await
        .with_context(|| {
            format!("Failed to delete notification details for {kind} from storage")
        })?;

        Ok(res.rows_affected())
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    async fn delete_notifications<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        kind: NotificationSourceKind,
    ) -> Result<u64, UniversalInboxError> {
        let res = sqlx::query!(
            r#"
            DELETE FROM notification
            WHERE notification.kind = $1
            "#,
            kind.to_string()
        )
        .execute(&mut **executor)
        .await
        .with_context(|| format!("Failed to delete notification for {kind} from storage"))?;

        Ok(res.rows_affected())
    }
}

#[derive(sqlx::Type, Debug)]
#[sqlx(type_name = "notification_status")]
enum PgNotificationStatus {
    Unread,
    Read,
    Deleted,
    Unsubscribed,
}

impl TryFrom<&PgNotificationStatus> for NotificationStatus {
    type Error = UniversalInboxError;

    fn try_from(status: &PgNotificationStatus) -> Result<Self, Self::Error> {
        let status_str = format!("{status:?}");
        status_str
            .parse()
            .map_err(|e| UniversalInboxError::InvalidEnumData {
                source: e,
                output: status_str,
            })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct NotificationRow {
    id: Uuid,
    title: String,
    status: PgNotificationStatus,
    source_id: String,
    source_html_url: Option<String>,
    metadata: Json<NotificationMetadata>,
    updated_at: NaiveDateTime,
    last_read_at: Option<NaiveDateTime>,
    snoozed_until: Option<NaiveDateTime>,
    user_id: Uuid,
    details: Option<Json<NotificationDetails>>,
    task_id: Option<Uuid>,
}

#[derive(Debug)]
struct NotificationWithTaskRow {
    notification_id: Uuid,
    notification_title: String,
    notification_status: PgNotificationStatus,
    notification_source_id: String,
    notification_source_html_url: Option<String>,
    notification_metadata: Json<NotificationMetadata>,
    notification_updated_at: NaiveDateTime,
    notification_last_read_at: Option<NaiveDateTime>,
    notification_snoozed_until: Option<NaiveDateTime>,
    notification_user_id: Uuid,
    notification_details: Option<Json<NotificationDetails>>,
    task_row: Option<TaskRow>,
}

impl FromRow<'_, PgRow> for NotificationWithTaskRow {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(NotificationWithTaskRow {
            notification_id: row.try_get("notification_id")?,
            notification_title: row.try_get("notification_title")?,
            notification_status: row
                .try_get::<PgNotificationStatus, &str>("notification_status")?,
            notification_source_id: row.try_get("notification_source_id")?,
            notification_source_html_url: row.try_get("notification_source_html_url")?,
            notification_metadata: row.try_get("notification_metadata")?,
            notification_updated_at: row.try_get("notification_updated_at")?,
            notification_last_read_at: row.try_get("notification_last_read_at")?,
            notification_snoozed_until: row.try_get("notification_snoozed_until")?,
            notification_user_id: row.try_get("notification_user_id")?,
            notification_details: row.try_get("notification_details")?,
            task_row: row
                .try_get::<Option<Uuid>, &str>("id")?
                .map(|_task_id| TaskRow::from_row(row))
                .transpose()?,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct UpdatedNotificationRow {
    #[sqlx(flatten)]
    pub notification_row: NotificationRow,
    pub is_updated: bool,
}

impl TryFrom<NotificationRow> for Notification {
    type Error = UniversalInboxError;

    fn try_from(row: NotificationRow) -> Result<Self, Self::Error> {
        (&row).try_into()
    }
}

impl TryFrom<&NotificationRow> for Notification {
    type Error = UniversalInboxError;

    fn try_from(row: &NotificationRow) -> Result<Self, Self::Error> {
        let status = (&row.status).try_into()?;
        let source_html_url = row
            .source_html_url
            .as_ref()
            .map(|url| {
                url.parse::<Url>()
                    .map_err(|e| UniversalInboxError::InvalidUrlData {
                        source: e,
                        output: url.clone(),
                    })
            })
            .transpose()?;

        Ok(Notification {
            id: row.id.into(),
            title: row.title.to_string(),
            status,
            source_id: row.source_id.clone(),
            source_html_url,
            metadata: row.metadata.0.clone(),
            updated_at: DateTime::from_naive_utc_and_offset(row.updated_at, Utc),
            last_read_at: row
                .last_read_at
                .map(|last_read_at| DateTime::from_naive_utc_and_offset(last_read_at, Utc)),
            snoozed_until: row
                .snoozed_until
                .map(|snoozed_until| DateTime::from_naive_utc_and_offset(snoozed_until, Utc)),
            user_id: row.user_id.into(),
            details: row.details.as_ref().map(|details| details.0.clone()),
            task_id: row.task_id.map(|task_id| task_id.into()),
        })
    }
}

impl TryFrom<&NotificationWithTaskRow> for NotificationWithTask {
    type Error = UniversalInboxError;

    fn try_from(row: &NotificationWithTaskRow) -> Result<Self, Self::Error> {
        let status = (&row.notification_status).try_into()?;
        let source_html_url = row
            .notification_source_html_url
            .as_ref()
            .map(|url| {
                url.parse::<Url>()
                    .map_err(|e| UniversalInboxError::InvalidUrlData {
                        source: e,
                        output: url.clone(),
                    })
            })
            .transpose()?;

        Ok(NotificationWithTask {
            id: row.notification_id.into(),
            title: row.notification_title.to_string(),
            status,
            source_id: row.notification_source_id.clone(),
            source_html_url,
            metadata: row.notification_metadata.0.clone(),
            updated_at: DateTime::from_naive_utc_and_offset(row.notification_updated_at, Utc),
            last_read_at: row
                .notification_last_read_at
                .map(|last_read_at| DateTime::from_naive_utc_and_offset(last_read_at, Utc)),
            snoozed_until: row
                .notification_snoozed_until
                .map(|snoozed_until| DateTime::from_naive_utc_and_offset(snoozed_until, Utc)),
            user_id: row.notification_user_id.into(),
            details: row
                .notification_details
                .as_ref()
                .map(|details| details.0.clone()),
            task: row
                .task_row
                .as_ref()
                .map(|task_row| task_row.try_into())
                .transpose()?,
        })
    }
}
