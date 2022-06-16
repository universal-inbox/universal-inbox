use async_trait::async_trait;
use universal_inbox::Notification;
use uuid::Uuid;

pub mod notification_service;

#[async_trait]
pub trait NotificationRepository: Send + Sync {
    async fn get_one(&self, id: uuid::Uuid) -> Result<Option<Notification>, UniversalInboxError>;
    async fn fetch_all(&self) -> Result<Vec<Notification>, UniversalInboxError>;
    async fn create(
        &self,
        notification: &Notification,
    ) -> Result<Notification, UniversalInboxError>;
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
    InvalidData {
        #[source]
        source: enum_derive::ParseEnumError,
        output: String,
    },
    #[error("The entity {id} already exists")]
    AlreadyExists {
        #[source]
        source: sqlx::Error,
        id: Uuid,
    },
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}