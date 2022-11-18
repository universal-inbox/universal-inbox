use std::sync::Arc;

use anyhow::{anyhow, Context};
use chrono::{DateTime, NaiveDateTime, Utc};
use duplicate::duplicate_item;
use http::Uri;
use sqlx::{pool::PoolConnection, types::Json, PgPool, Postgres, Transaction};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::universal_inbox::{UniversalInboxError, UpdateStatus};
use universal_inbox::{
    integrations::github::GithubNotification, Notification, NotificationPatch, NotificationStatus,
};

pub struct NotificationRepository {
    pub pool: Box<PgPool>,
}

impl NotificationRepository {
    pub fn new(pool: PgPool) -> NotificationRepository {
        NotificationRepository {
            pool: Box::new(pool),
        }
    }

    pub async fn connect(
        &self,
    ) -> Result<Arc<ConnectedNotificationRepository>, UniversalInboxError> {
        let connection = self
            .pool
            .acquire()
            .await
            .context("Failed to connection to the database")?;
        Ok(Arc::new(ConnectedNotificationRepository {
            executor: Arc::new(Mutex::new(connection)),
        }))
    }

    pub async fn begin(
        &self,
    ) -> Result<Arc<TransactionalNotificationRepository>, UniversalInboxError> {
        let transaction = self
            .pool
            .begin()
            .await
            .context("Failed to begin database transaction")?;
        Ok(Arc::new(TransactionalNotificationRepository {
            executor: Arc::new(Mutex::new(transaction)),
        }))
    }
}

pub struct ConnectedNotificationRepository {
    pub executor: Arc<Mutex<PoolConnection<Postgres>>>,
}

pub struct TransactionalNotificationRepository<'a> {
    pub executor: Arc<Mutex<Transaction<'a, Postgres>>>,
}

impl<'a> TransactionalNotificationRepository<'a> {
    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn commit(self) -> Result<(), UniversalInboxError> {
        let transaction = Arc::try_unwrap(self.executor)
            .map_err(|_| {
                UniversalInboxError::Unexpected(anyhow!(
                    "Cannot extract transaction to commit it as it has other references using it"
                ))
            })?
            .into_inner();
        Ok(transaction
            .commit()
            .await
            .context("Failed to commit database transaction")?)
    }
}

#[duplicate_item(repository; [ConnectedNotificationRepository]; [TransactionalNotificationRepository<'a>])]
impl<'a> repository {
    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn get_one(&self, id: Uuid) -> Result<Option<Notification>, UniversalInboxError> {
        let mut executor = self.executor.lock().await;
        let row = sqlx::query_as!(
            NotificationRow,
            r#"
                SELECT
                  id,
                  title,
                  kind,
                  status,
                  source_id,
                  source_html_url,
                  metadata as "metadata: Json<GithubNotification>",
                  updated_at,
                  last_read_at
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
    pub async fn fetch_all(
        &self,
        status: NotificationStatus,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        let mut executor = self.executor.lock().await;
        let records = sqlx::query_as!(
            NotificationRow,
            r#"
                SELECT
                  id,
                  title,
                  kind,
                  status,
                  source_id,
                  source_html_url,
                  metadata as "metadata: Json<GithubNotification>",
                  updated_at,
                  last_read_at
                FROM
                  notification
                WHERE
                  status = $1
            "#,
            status.to_string()
        )
        .fetch_all(&mut *executor)
        .await
        .context("Failed to fetch notifications from storage")?;

        records.iter().map(|r| r.try_into()).collect()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn create(
        &self,
        notification: Box<Notification>,
    ) -> Result<Notification, UniversalInboxError> {
        let mut executor = self.executor.lock().await;
        let metadata = Json(notification.metadata.clone());
        let kind = notification.kind.to_string();

        sqlx::query!(
            r#"
                INSERT INTO notification
                  (
                    id,
                    title,
                    kind,
                    status,
                    source_id,
                    source_html_url,
                    metadata,
                    updated_at,
                    last_read_at
                  )
                VALUES
                  ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            notification.id,
            notification.title,
            kind,
            notification.status.to_string(),
            notification.source_id,
            notification
                .source_html_url
                .as_ref()
                .map(|url| url.to_string()),
            metadata as Json<GithubNotification>, // force the macro to ignore type checking
            notification.updated_at.naive_utc(),
            notification
                .last_read_at
                .map(|last_read_at| last_read_at.naive_utc())
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

        Ok(*notification.clone())
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn update_stale_notifications_status_from_source_ids(
        &self,
        active_source_notification_ids: Vec<String>,
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
                  AND (status = 'Read' OR status = 'Unread')
                RETURNING
                  id,
                  title,
                  kind,
                  status,
                  source_id,
                  source_html_url,
                  metadata as "metadata: Json<GithubNotification>",
                  updated_at,
                  last_read_at
            "#,
            status.to_string(),
            &active_source_notification_ids[..]
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
    pub async fn create_or_update(
        &self,
        notification: Box<Notification>,
    ) -> Result<Notification, UniversalInboxError> {
        let mut executor = self.executor.lock().await;
        let metadata = Json(notification.metadata.clone());
        let kind = notification.kind.to_string();

        let id: Uuid = sqlx::query_scalar!(
            r#"
                INSERT INTO notification
                  (
                    id,
                    title,
                    kind,
                    status,
                    source_id,
                    source_html_url,
                    metadata,
                    updated_at,
                    last_read_at
                  )
                VALUES
                  ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                ON CONFLICT (source_id) DO UPDATE
                SET
                  title = $2,
                  kind = $3,
                  status = $4,
                  source_html_url = $6,
                  metadata = $7,
                  updated_at = $8,
                  last_read_at = $9
                RETURNING
                  id
            "#,
            notification.id,
            notification.title,
            kind,
            notification.status.to_string(),
            notification.source_id,
            notification
                .source_html_url
                .as_ref()
                .map(|url| url.to_string()),
            metadata as Json<GithubNotification>, // force the macro to ignore type checking
            notification.updated_at.naive_utc(),
            notification
                .last_read_at
                .map(|last_read_at| last_read_at.naive_utc())
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
    pub async fn update<'b>(
        &self,
        notification_id: Uuid,
        patch: &'b NotificationPatch,
    ) -> Result<UpdateStatus<Box<Notification>>, UniversalInboxError> {
        let status = patch.status.ok_or_else(|| {
            UniversalInboxError::MissingInputData(
                "Missing `status` field value to update notification {notification_id}".to_string(),
            )
        })?;
        let mut executor = self.executor.lock().await;

        let record: Option<UpdatedNotificationRow> = sqlx::query_as(
            r#"
                UPDATE
                  notification
                SET
                  status = $2
                WHERE
                  id = $1
                RETURNING
                  id,
                  title,
                  kind,
                  status,
                  source_id,
                  source_html_url,
                  metadata,
                  updated_at,
                  last_read_at,
                  (SELECT status != $2 FROM notification WHERE id = $1) as "is_status_updated"
            "#,
        )
        .bind(notification_id)
        .bind(status.to_string())
        .fetch_optional(&mut *executor)
        .await
        .context(format!(
            "Failed to update notification {} from storage",
            notification_id
        ))?;

        if let Some(updated_notification_row) = record {
            Ok(UpdateStatus {
                updated: updated_notification_row.is_status_updated,
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
    kind: String,
    status: String,
    source_id: String,
    source_html_url: Option<String>,
    metadata: Json<GithubNotification>,
    updated_at: NaiveDateTime,
    last_read_at: Option<NaiveDateTime>,
}

#[derive(Debug, sqlx::FromRow)]
struct UpdatedNotificationRow {
    #[sqlx(flatten)]
    pub notification_row: NotificationRow,
    pub is_status_updated: bool,
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
        let kind = row
            .kind
            .parse()
            .map_err(|e| UniversalInboxError::InvalidEnumData {
                source: e,
                output: row.kind.clone(),
            })?;
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
            kind,
            status,
            source_id: row.source_id.clone(),
            source_html_url,
            metadata: row.metadata.0.clone(),
            updated_at: DateTime::<Utc>::from_utc(row.updated_at, Utc),
            last_read_at: row
                .last_read_at
                .map(|last_read_at| DateTime::<Utc>::from_utc(last_read_at, Utc)),
        })
    }
}
