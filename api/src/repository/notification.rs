use crate::universal_inbox::{NotificationRepository, UniversalInboxError};
use anyhow::{anyhow, Context};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::{types::Json, PgPool};
use universal_inbox::{GithubNotification, Notification};

pub struct PgRepository {
    pool: PgPool,
}

impl PgRepository {
    pub fn new(pool: PgPool) -> PgRepository {
        PgRepository { pool }
    }
}

#[derive(Debug)]
struct NotificationRow {
    id: uuid::Uuid,
    title: String,
    kind: String,
    status: String,
    metadata: Json<GithubNotification>,
    updated_at: NaiveDateTime,
    last_read_at: Option<NaiveDateTime>,
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
            .map_err(|e| UniversalInboxError::InvalidData {
                source: e,
                output: row.kind.clone(),
            })?;
        let status = row
            .status
            .parse()
            .map_err(|e| UniversalInboxError::InvalidData {
                source: e,
                output: row.status.clone(),
            })?;

        Ok(Notification {
            id: row.id,
            title: row.title.to_string(),
            kind,
            status,
            metadata: row.metadata.0.clone(),
            updated_at: DateTime::<Utc>::from_utc(row.updated_at, Utc),
            last_read_at: row
                .last_read_at
                .map(|last_read_at| DateTime::<Utc>::from_utc(last_read_at, Utc)),
        })
    }
}

#[async_trait]
impl NotificationRepository for PgRepository {
    #[tracing::instrument(level = "debug", skip(self))]
    async fn get_one(&self, id: uuid::Uuid) -> Result<Option<Notification>, UniversalInboxError> {
        let row = sqlx::query_as!(
            NotificationRow,
            r#"SELECT id, title, kind, status, metadata as "metadata: Json<GithubNotification>", updated_at, last_read_at FROM notification WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.pool.clone())
        .await
        .with_context(|| format!("Failed to fetch notification {} from storage", id))?;

        row.map(|notification_row| notification_row.try_into())
            .transpose()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn fetch_all(&self) -> Result<Vec<Notification>, UniversalInboxError> {
        let records = sqlx::query_as!(
            NotificationRow,
            r#"SELECT id, title, kind, status, metadata as "metadata: Json<GithubNotification>", updated_at, last_read_at FROM notification"#
        )
        .fetch_all(&self.pool.clone())
        .await
        .context("Failed to fetch notifications from storage")?;

        records.iter().map(|r| r.try_into()).collect()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn create(
        &self,
        notification: &Notification,
    ) -> Result<Notification, UniversalInboxError> {
        let metadata = Json(notification.metadata.clone());
        let kind = notification.kind.to_string();

        sqlx::query!(
            r#"
          INSERT INTO notification
            (id, title, kind, status, metadata, updated_at, last_read_at)
          VALUES
            ($1, $2, $3, $4, $5, $6, $7)
        "#,
            notification.id,
            notification.title,
            kind,
            notification.status.to_string(),
            metadata as Json<GithubNotification>, // force the macro to ignore type checking
            notification.updated_at.naive_utc(),
            notification
                .last_read_at
                .map(|last_read_at| last_read_at.naive_utc())
        )
        .execute(&self.pool.clone())
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

        Ok(notification.clone())
    }
}
