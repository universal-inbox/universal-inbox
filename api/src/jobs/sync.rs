use std::sync::Arc;

use apalis::prelude::Data;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;

use universal_inbox::{
    notification::NotificationSyncSourceKind, task::TaskSyncSourceKind, user::UserId,
};

use crate::universal_inbox::{
    notification::service::NotificationService, task::service::TaskService, UniversalInboxError,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncNotificationsJob {
    pub source: Option<NotificationSyncSourceKind>,
    pub user_id: Option<UserId>,
}

#[tracing::instrument(level = "debug", skip(event, notification_service), err)]
pub async fn handle_sync_notifications(
    event: SyncNotificationsJob,
    notification_service: Data<Arc<RwLock<NotificationService>>>,
) -> Result<(), UniversalInboxError> {
    let source_kind_string = event
        .source
        .map(|s| s.to_string())
        .unwrap_or_else(|| "all types of".to_string());
    let service = notification_service.read().await;
    if let Some(user_id) = event.user_id {
        info!("Syncing {source_kind_string} notifications for user {user_id}");

        if let Some(source) = event.source {
            service
                .sync_notifications_with_transaction(source, user_id, false)
                .await?;
        } else {
            service.sync_all_notifications(user_id, false).await?;
        };
    } else {
        info!("Syncing {source_kind_string} notifications for all users");

        service
            .sync_notifications_for_all_users(event.source, false)
            .await?;
    }

    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncTasksJob {
    pub source: Option<TaskSyncSourceKind>,
    pub user_id: Option<UserId>,
}

#[tracing::instrument(level = "debug", skip(event, task_service), err)]
pub async fn handle_sync_tasks(
    event: SyncTasksJob,
    task_service: Data<Arc<RwLock<TaskService>>>,
) -> Result<(), UniversalInboxError> {
    let source_kind_string = event
        .source
        .map(|s| s.to_string())
        .unwrap_or_else(|| "all types of".to_string());
    let service = task_service.read().await;
    if let Some(user_id) = event.user_id {
        info!("Syncing {source_kind_string} tasks for user {user_id}");

        if let Some(source) = event.source {
            service
                .sync_tasks_with_transaction(source, user_id, false)
                .await?;
        } else {
            service.sync_all_tasks(user_id, false).await?;
        };
    } else {
        info!("Syncing {source_kind_string} tasks for all users");

        service
            .sync_tasks_for_all_users(event.source, false)
            .await?;
    }

    Ok(())
}
