use std::sync::Arc;

use anyhow::Context;

use crate::{
    integrations::{notification::NotificationSyncSourceKind, task::TaskSyncSourceKind},
    universal_inbox::{
        notification::service::NotificationService, task::service::TaskService, UniversalInboxError,
    },
};

pub async fn sync_notifications(
    service: Arc<NotificationService>,
    source: &Option<NotificationSyncSourceKind>,
) -> Result<(), UniversalInboxError> {
    let transactional_service = service.begin().await.context(format!(
        "Failed to create new transaction while syncing {:?}",
        &source
    ))?;

    transactional_service
        .sync_notifications(source)
        .await
        .context(format!("Failed to sync {:?}", &source))?;

    transactional_service
        .commit()
        .await
        .context(format!("Failed to commit while syncing {:?}", &source))?;

    Ok(())
}

pub async fn sync_tasks(
    service: Arc<TaskService>,
    source: &Option<TaskSyncSourceKind>,
) -> Result<(), UniversalInboxError> {
    let transactional_service = service.begin().await.context(format!(
        "Failed to create new transaction while syncing {:?}",
        &source
    ))?;

    transactional_service
        .sync_tasks(source)
        .await
        .context(format!("Failed to sync {:?}", &source))?;

    transactional_service
        .commit()
        .await
        .context(format!("Failed to commit while syncing {:?}", &source))?;

    Ok(())
}
