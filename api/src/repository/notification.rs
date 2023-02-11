use anyhow::{anyhow, Context};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use http::Uri;
use sqlx::{postgres::PgRow, types::Json, FromRow, Postgres, QueryBuilder, Row, Transaction};
use uuid::Uuid;

use universal_inbox::{
    notification::{
        Notification, NotificationId, NotificationMetadata, NotificationPatch, NotificationStatus,
        NotificationWithTask,
    },
    task::TaskId,
};

use crate::{
    integrations::notification::NotificationSourceKind,
    repository::{task::TaskRow, Repository},
    universal_inbox::{UniversalInboxError, UpdateStatus},
};

#[async_trait]
pub trait NotificationRepository {
    async fn get_one_notification<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        id: NotificationId,
    ) -> Result<Option<Notification>, UniversalInboxError>;
    async fn fetch_all_notifications<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        status: NotificationStatus,
        include_snoozed_notifications: bool,
        task_id: Option<TaskId>,
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
    ) -> Result<Vec<Notification>, UniversalInboxError>;
    async fn create_or_update_notification<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification: Box<Notification>,
    ) -> Result<Notification, UniversalInboxError>;
    async fn update_notification<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification_id: NotificationId,
        patch: &NotificationPatch,
    ) -> Result<UpdateStatus<Box<Notification>>, UniversalInboxError>;
    async fn update_notifications_for_task<'a, 'b>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task_id: TaskId,
        notification_kind: Option<NotificationSourceKind>,
        patch: &'b NotificationPatch,
    ) -> Result<Vec<UpdateStatus<Notification>>, UniversalInboxError>;
}

#[async_trait]
impl NotificationRepository for Repository {
    #[tracing::instrument(level = "debug", skip(self))]
    async fn get_one_notification<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        id: NotificationId,
    ) -> Result<Option<Notification>, UniversalInboxError> {
        let row = sqlx::query_as!(
            NotificationRow,
            r#"
                SELECT
                  id,
                  title,
                  status,
                  source_id,
                  source_html_url,
                  metadata as "metadata: Json<NotificationMetadata>",
                  updated_at,
                  last_read_at,
                  snoozed_until,
                  task_id,
                  task_source_id
                FROM notification
                WHERE id = $1
            "#,
            id.0
        )
        .fetch_optional(executor)
        .await
        .with_context(|| format!("Failed to fetch notification {} from storage", id))?;

        row.map(|notification_row| notification_row.try_into())
            .transpose()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn fetch_all_notifications<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        status: NotificationStatus,
        include_snoozed_notifications: bool,
        task_id: Option<TaskId>,
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
                  notification.task_source_id as notification_task_source_id,
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
                  task.metadata
                FROM
                  notification
                LEFT JOIN task ON task.id = notification.task_id
                WHERE
                  notification.status =
            "#,
        );

        query_builder.push_bind(status.to_string());

        if !include_snoozed_notifications {
            query_builder
                .push(" AND (notification.snoozed_until is NULL OR notification.snoozed_until <=")
                .push_bind(Utc::now().naive_utc())
                .push(")");
        }

        if let Some(id) = task_id {
            query_builder
                .push(" AND notification.task_id = ")
                .push_bind(id.0);
        }

        let records = query_builder
            .build_query_as::<NotificationWithTaskRow>()
            .fetch_all(executor)
            .await
            .context("Failed to fetch notifications from storage")?;

        records.iter().map(|r| r.try_into()).collect()
    }

    #[tracing::instrument(level = "debug", skip(self))]
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
                    task_id,
                    task_source_id
                  )
                VALUES
                  ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
            notification.id.0,
            notification.title,
            notification.status.to_string(),
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
            notification.task_id.map(|task_id| task_id.0),
            notification.task_source_id
        )
        .execute(executor)
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

    #[tracing::instrument(level = "debug", skip(self))]
    async fn update_stale_notifications_status_from_source_ids<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        active_source_notification_ids: Vec<String>,
        kind: NotificationSourceKind,
        status: NotificationStatus,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        let records = sqlx::query_as!(
            NotificationRow,
            r#"
                UPDATE
                  notification
                SET
                  status = $1
                WHERE
                  NOT source_id = ANY($2)
                  AND kind = $3
                  AND (status = 'Read' OR status = 'Unread')
                RETURNING
                  id,
                  title,
                  status,
                  source_id,
                  source_html_url,
                  metadata as "metadata: Json<NotificationMetadata>",
                  updated_at,
                  last_read_at,
                  snoozed_until,
                  task_id,
                  task_source_id
            "#,
            status.to_string(),
            &active_source_notification_ids[..],
            kind.to_string(),
        )
        .fetch_all(executor)
        .await
        .context("Failed to update stale notification status from storage")?;

        records
            .iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<Notification>, UniversalInboxError>>()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn create_or_update_notification<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification: Box<Notification>,
    ) -> Result<Notification, UniversalInboxError> {
        let metadata = Json(notification.metadata.clone());

        let id: NotificationId = NotificationId(
            sqlx::query_scalar!(
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
                    task_id,
                    task_source_id
                  )
                VALUES
                  ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                ON CONFLICT (source_id, kind) DO UPDATE
                SET
                  title = $2,
                  status = $3,
                  source_html_url = $5,
                  metadata = $6,
                  updated_at = $7,
                  last_read_at = $8,
                  snoozed_until = $9,
                  task_id = $10,
                  task_source_id = $11
                RETURNING
                  id
            "#,
                notification.id.0,
                notification.title,
                notification.status.to_string(),
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
                notification.task_id.map(|task_id| task_id.0),
                notification.task_source_id
            )
            .fetch_one(executor)
            .await
            .with_context(|| {
                format!(
                    "Failed to update notification with source ID {} from storage",
                    notification.source_id
                )
            })?,
        );

        Ok(Notification {
            id,
            ..*notification
        })
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn update_notification<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification_id: NotificationId,
        patch: &NotificationPatch,
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
                .push_bind_unseparated(status.to_string());
        }
        if let Some(snoozed_until) = patch.snoozed_until {
            separated
                .push(" snoozed_until = ")
                .push_bind_unseparated(snoozed_until.naive_utc());
        }

        query_builder
            .push(" WHERE id = ")
            .push_bind(notification_id.0);
        query_builder.push(
            r#"
                RETURNING
                  id,
                  title,
                  status,
                  source_id,
                  source_html_url,
                  metadata,
                  updated_at,
                  last_read_at,
                  snoozed_until,
                  task_id,
                  task_source_id,
                  (SELECT"#,
        );

        let mut separated = query_builder.separated(" OR ");
        if let Some(status) = patch.status {
            separated
                .push(" status != ")
                .push_bind_unseparated(status.to_string());
        }
        if let Some(snoozed_until) = patch.snoozed_until {
            separated
                .push(" (snoozed_until is NULL OR snoozed_until != ")
                .push_bind_unseparated(snoozed_until.naive_utc())
                .push_unseparated(")");
        }

        query_builder
            .push(" FROM notification WHERE id = ")
            .push_bind(notification_id.0);
        query_builder.push(r#") as "is_updated""#);

        let record: Option<UpdatedNotificationRow> = query_builder
            .build_query_as::<UpdatedNotificationRow>()
            .fetch_optional(executor)
            .await
            .context(format!(
                "Failed to update notification {} from storage",
                notification_id
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

    #[tracing::instrument(level = "debug", skip(self))]
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
                .push_bind_unseparated(status.to_string());
        }
        if let Some(snoozed_until) = patch.snoozed_until {
            separated
                .push(" snoozed_until = ")
                .push_bind_unseparated(snoozed_until.naive_utc());
        }

        query_builder.push(" WHERE task_id = ").push_bind(task_id.0);

        if let Some(kind) = notification_kind {
            query_builder
                .push(" AND kind = ")
                .push_bind(kind.to_string());
        }

        query_builder.push(
            r#"
                RETURNING
                  id,
                  title,
                  status,
                  source_id,
                  source_html_url,
                  metadata,
                  updated_at,
                  last_read_at,
                  snoozed_until,
                  task_id,
                  task_source_id,
                  (SELECT"#,
        );

        let mut separated = query_builder.separated(" AND ");
        if let Some(status) = patch.status {
            separated
                .push(" status != ")
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
            .fetch_all(executor)
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
}

#[derive(Debug, sqlx::FromRow)]
struct NotificationRow {
    id: Uuid,
    title: String,
    status: String,
    source_id: String,
    source_html_url: Option<String>,
    metadata: Json<NotificationMetadata>,
    updated_at: NaiveDateTime,
    last_read_at: Option<NaiveDateTime>,
    snoozed_until: Option<NaiveDateTime>,
    task_id: Option<Uuid>,
    task_source_id: Option<String>,
}

#[derive(Debug)]
struct NotificationWithTaskRow {
    notification_id: Uuid,
    notification_title: String,
    notification_status: String,
    notification_source_id: String,
    notification_source_html_url: Option<String>,
    notification_metadata: Json<NotificationMetadata>,
    notification_updated_at: NaiveDateTime,
    notification_last_read_at: Option<NaiveDateTime>,
    notification_snoozed_until: Option<NaiveDateTime>,
    task_row: Option<TaskRow>,
    notification_task_source_id: Option<String>,
}

impl FromRow<'_, PgRow> for NotificationWithTaskRow {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(NotificationWithTaskRow {
            notification_id: row.try_get("notification_id")?,
            notification_title: row.try_get("notification_title")?,
            notification_status: row.try_get("notification_status")?,
            notification_source_id: row.try_get("notification_source_id")?,
            notification_source_html_url: row.try_get("notification_source_html_url")?,
            notification_metadata: row.try_get("notification_metadata")?,
            notification_updated_at: row.try_get("notification_updated_at")?,
            notification_last_read_at: row.try_get("notification_last_read_at")?,
            notification_snoozed_until: row.try_get("notification_snoozed_until")?,
            task_row: row
                .try_get::<Option<Uuid>, &str>("id")?
                .map(|_task_id| TaskRow::from_row(row))
                .transpose()?,
            notification_task_source_id: row.try_get("notification_task_source_id")?,
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
        let status = row
            .status
            .parse()
            .map_err(|e| UniversalInboxError::InvalidEnumData {
                source: e,
                output: row.status.clone(),
            })?;
        let source_html_url = row
            .source_html_url
            .as_ref()
            .map(|url| {
                url.parse::<Uri>()
                    .map_err(|e| UniversalInboxError::InvalidUriData {
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
            updated_at: DateTime::<Utc>::from_utc(row.updated_at, Utc),
            last_read_at: row
                .last_read_at
                .map(|last_read_at| DateTime::<Utc>::from_utc(last_read_at, Utc)),
            snoozed_until: row
                .snoozed_until
                .map(|snoozed_until| DateTime::<Utc>::from_utc(snoozed_until, Utc)),
            task_id: row.task_id.map(|task_id| task_id.into()),
            task_source_id: row.task_source_id.clone(),
        })
    }
}

impl TryFrom<&NotificationWithTaskRow> for NotificationWithTask {
    type Error = UniversalInboxError;

    fn try_from(row: &NotificationWithTaskRow) -> Result<Self, Self::Error> {
        let status =
            row.notification_status
                .parse()
                .map_err(|e| UniversalInboxError::InvalidEnumData {
                    source: e,
                    output: row.notification_status.clone(),
                })?;
        let source_html_url = row
            .notification_source_html_url
            .as_ref()
            .map(|url| {
                url.parse::<Uri>()
                    .map_err(|e| UniversalInboxError::InvalidUriData {
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
            updated_at: DateTime::<Utc>::from_utc(row.notification_updated_at, Utc),
            last_read_at: row
                .notification_last_read_at
                .map(|last_read_at| DateTime::<Utc>::from_utc(last_read_at, Utc)),
            snoozed_until: row
                .notification_snoozed_until
                .map(|snoozed_until| DateTime::<Utc>::from_utc(snoozed_until, Utc)),
            task: row
                .task_row
                .as_ref()
                .map(|task_row| task_row.try_into())
                .transpose()?,
            task_source_id: row.notification_task_source_id.clone(),
        })
    }
}
