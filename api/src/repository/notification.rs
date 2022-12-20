use anyhow::{anyhow, Context};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use duplicate::duplicate_item;
use http::Uri;
use sqlx::{types::Json, QueryBuilder};
use uuid::Uuid;

use universal_inbox::notification::{
    Notification, NotificationMetadata, NotificationPatch, NotificationStatus,
};

use crate::{
    integrations::notification::NotificationSourceKind,
    universal_inbox::{UniversalInboxError, UpdateStatus},
};

use super::{ConnectedRepository, TransactionalRepository};

#[async_trait]
pub trait NotificationRepository {
    async fn get_one_notification(
        &self,
        id: Uuid,
    ) -> Result<Option<Notification>, UniversalInboxError>;
    async fn fetch_all_notifications(
        &self,
        status: NotificationStatus,
        include_snoozed_notifications: bool,
        task_id: Option<Uuid>,
    ) -> Result<Vec<Notification>, UniversalInboxError>;
    async fn create_notification(
        &self,
        notification: Box<Notification>,
    ) -> Result<Box<Notification>, UniversalInboxError>;
    async fn update_stale_notifications_status_from_source_ids(
        &self,
        active_source_notification_ids: Vec<String>,
        kind: NotificationSourceKind,
        status: NotificationStatus,
    ) -> Result<Vec<Notification>, UniversalInboxError>;
    async fn create_or_update_notification(
        &self,
        notification: Box<Notification>,
    ) -> Result<Notification, UniversalInboxError>;
    async fn update_notification<'b>(
        &self,
        notification_id: Uuid,
        patch: &'b NotificationPatch,
    ) -> Result<UpdateStatus<Box<Notification>>, UniversalInboxError>;
}

#[duplicate_item(repository; [ConnectedRepository]; [TransactionalRepository<'a>])]
#[async_trait]
#[allow(clippy::extra_unused_lifetimes)]
impl<'a> NotificationRepository for repository {
    #[tracing::instrument(level = "debug", skip(self))]
    async fn get_one_notification(
        &self,
        id: Uuid,
    ) -> Result<Option<Notification>, UniversalInboxError> {
        let mut executor = self.executor.lock().await;
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
            id
        )
        .fetch_optional(&mut *executor)
        .await
        .with_context(|| format!("Failed to fetch notification {} from storage", id))?;

        row.map(|notification_row| notification_row.try_into())
            .transpose()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn fetch_all_notifications(
        &self,
        status: NotificationStatus,
        include_snoozed_notifications: bool,
        task_id: Option<Uuid>,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        let mut executor = self.executor.lock().await;

        let mut query_builder = QueryBuilder::new(
            r#"
                SELECT
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
                FROM
                  notification
                WHERE
                  status =
            "#,
        );
        query_builder.push_bind(status.to_string());

        if !include_snoozed_notifications {
            query_builder
                .push(" AND (snoozed_until is NULL OR snoozed_until <=")
                .push_bind(Utc::now().naive_utc())
                .push(")");
        }

        if let Some(id) = task_id {
            query_builder.push(" AND task_id = ").push_bind(id);
        }

        let records = query_builder
            .build_query_as::<NotificationRow>()
            .fetch_all(&mut *executor)
            .await
            .context("Failed to fetch notifications from storage")?;

        records.iter().map(|r| r.try_into()).collect()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn create_notification(
        &self,
        notification: Box<Notification>,
    ) -> Result<Box<Notification>, UniversalInboxError> {
        let mut executor = self.executor.lock().await;
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
            notification.id,
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
            notification.task_id,
            notification.task_source_id
        )
        .execute(&mut *executor)
        .await
        .map_err(|e| {
            match e
                .as_database_error()
                .and_then(|db_error| db_error.code().map(|code| code.to_string()))
            {
                Some(x) if x == *"23505" => UniversalInboxError::AlreadyExists {
                    source: e,
                    id: notification.id,
                },
                _ => UniversalInboxError::Unexpected(anyhow!(
                    "Failed to insert new notification into storage"
                )),
            }
        })?;

        Ok(notification)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn update_stale_notifications_status_from_source_ids(
        &self,
        active_source_notification_ids: Vec<String>,
        kind: NotificationSourceKind,
        status: NotificationStatus,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        let mut executor = self.executor.lock().await;
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
        .fetch_all(&mut *executor)
        .await
        .context("Failed to update stale notification status from storage")?;

        records
            .iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<Notification>, UniversalInboxError>>()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn create_or_update_notification(
        &self,
        notification: Box<Notification>,
    ) -> Result<Notification, UniversalInboxError> {
        let mut executor = self.executor.lock().await;
        let metadata = Json(notification.metadata.clone());

        let id: Uuid = sqlx::query_scalar!(
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
            notification.id,
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
            notification.task_id,
            notification.task_source_id
        )
        .fetch_one(&mut *executor)
        .await
        .with_context(|| {
            format!(
                "Failed to update notification with source ID {} from storage",
                notification.source_id
            )
        })?;

        Ok(Notification {
            id,
            ..*notification
        })
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn update_notification<'b>(
        &self,
        notification_id: Uuid,
        patch: &'b NotificationPatch,
    ) -> Result<UpdateStatus<Box<Notification>>, UniversalInboxError> {
        if *patch == Default::default() {
            return Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!(
                    "Missing `status` field value to update notification {notification_id}"
                ),
            });
        };

        let mut executor = self.executor.lock().await;

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
            .push_bind(notification_id);
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
            .push(" FROM notification WHERE id = ")
            .push_bind(notification_id);
        query_builder.push(r#") as "is_updated""#);

        let record: Option<UpdatedNotificationRow> = query_builder
            .build_query_as::<UpdatedNotificationRow>()
            .fetch_optional(&mut *executor)
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
            id: row.id,
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
            task_id: row.task_id,
            task_source_id: row.task_source_id.clone(),
        })
    }
}
