use std::sync::Arc;

use anyhow::Context;
use tokio::sync::RwLock;
use tracing::{error, info};

use universal_inbox::notification::NotificationSourceKind;

use crate::universal_inbox::{notification::service::NotificationService, UniversalInboxError};

#[tracing::instrument(
    name = "delete-notification-details",
    level = "info",
    skip(notification_service),
    err
)]
pub async fn delete_notification_details(
    notification_service: Arc<RwLock<NotificationService>>,
    source: NotificationSourceKind,
) -> Result<(), UniversalInboxError> {
    info!("Deleting notification details for source: {source}");

    let service = notification_service.read().await;
    let mut transaction = service.begin().await.context(format!(
        "Failed to create new transaction while deleting notification details for {source}"
    ))?;

    let result = service
        .delete_notification_details(&mut transaction, source)
        .await;

    match result {
        Ok(count) => {
            info!("{count} {source} notification details successfully deleted");
            transaction.commit().await.context(format!(
                "Failed to commit transaction while deleting {source} notification details"
            ))?;
            Ok(())
        }
        Err(err) => {
            error!("Failed to delete {source} notification details: {err:?}");
            transaction.rollback().await.context(format!(
                "Failed to rollback transaction while deleting {source} notification details"
            ))?;
            Err(err)
        }
    }
}
