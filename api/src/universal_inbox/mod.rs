use async_trait::async_trait;
use http::uri::InvalidUri;
use universal_inbox::{Notification, NotificationPatch, NotificationStatus};
use uuid::Uuid;

use crate::repository::database::TransactionalRepository;

pub mod notification;

#[async_trait]
pub trait NotificationRepository: Send + Sync + TransactionalRepository {
    async fn get_one(&self, id: uuid::Uuid) -> Result<Option<Notification>, UniversalInboxError>;
    async fn fetch_all(&self) -> Result<Vec<Notification>, UniversalInboxError>;
    async fn create(&self, notification: Notification)
        -> Result<Notification, UniversalInboxError>;
    async fn update_stale_notifications_status_from_source_ids(
        &self,
        active_source_notification_ids: Vec<String>,
        status: NotificationStatus,
    ) -> Result<Vec<Notification>, UniversalInboxError>;
    async fn create_or_update(
        &self,
        notification: Notification,
    ) -> Result<Notification, UniversalInboxError>;
    async fn update<'a>(
        &self,
        notification_id: Uuid,
        notification_update: &'a NotificationPatch,
    ) -> Result<UpdateStatus<Box<Notification>>, UniversalInboxError>;
}

fn error_chain_fmt(
    e: &impl std::error::Error,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    writeln!(f, "{}\n", e)?;
    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "Caused by:\n\t{}", cause)?;
        current = cause.source();
    }
    Ok(())
}

impl std::fmt::Debug for UniversalInboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

#[derive(thiserror::Error)]
pub enum UniversalInboxError {
    #[error("Error while parsing enum")]
    InvalidEnumData {
        #[source]
        source: enum_derive::ParseEnumError,
        output: String,
    },
    #[error("Error while parsing URI")]
    InvalidUriData {
        #[source]
        source: InvalidUri,
        output: String,
    },
    #[error("Missing input data: {0}")]
    MissingInputData(String),
    #[error("The entity {id} already exists")]
    AlreadyExists {
        #[source]
        source: sqlx::Error,
        id: Uuid,
    },
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

#[derive(Debug)]
pub struct UpdateStatus<T> {
    pub updated: bool,
    pub result: Option<T>,
}
