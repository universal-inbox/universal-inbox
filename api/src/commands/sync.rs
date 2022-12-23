use std::sync::Arc;

use anyhow::Context;
use tokio::sync::RwLock;

use crate::{
    integrations::{notification::NotificationSyncSourceKind, task::TaskSyncSourceKind},
    universal_inbox::{
        notification::service::NotificationService, task::service::TaskService, UniversalInboxError,
    },
};

pub async fn sync_notifications(
    notification_service: Arc<RwLock<NotificationService>>,
    source: &Option<NotificationSyncSourceKind>,
) -> Result<(), UniversalInboxError> {
    let service = notification_service.read().await;
    let mut transaction = service.begin().await.context(format!(
        "Failed to create new transaction while syncing {:?}",
        &source
    ))?;

    service
        .sync_notifications(&mut transaction, source)
        .await
        .context(format!("Failed to sync {:?}", &source))?;

    transaction
        .commit()
        .await
        .context(format!("Failed to commit while syncing {:?}", &source))?;

    Ok(())
}

pub async fn sync_tasks(
    task_service: Arc<RwLock<TaskService>>,
    source: &Option<TaskSyncSourceKind>,
) -> Result<(), UniversalInboxError> {
    let service = task_service.read().await;
    let mut transaction = service.begin().await.context(format!(
        "Failed to create new transaction while syncing {:?}",
        &source
    ))?;

    service
        .sync_tasks(&mut transaction, source)
        .await
        .context(format!("Failed to sync {:?}", &source))?;

    transaction
        .commit()
        .await
        .context(format!("Failed to commit while syncing {:?}", &source))?;

    Ok(())
}
