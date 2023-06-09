use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{error, info};

use universal_inbox::{notification::NotificationSyncSourceKind, task::TaskSyncSourceKind};

use crate::universal_inbox::{
    notification::service::NotificationService, task::service::TaskService, UniversalInboxError,
};

pub async fn sync_notifications_for_all_users(
    notification_service: Arc<RwLock<NotificationService>>,
    source: Option<NotificationSyncSourceKind>,
) -> Result<(), UniversalInboxError> {
    let source_kind_string = source
        .map(|s| s.to_string())
        .unwrap_or_else(|| "all types of".to_string());
    info!("Syncing {source_kind_string} notifications for all users");
    let service = notification_service.read().await;

    let result = service.sync_notifications_for_all_users(source).await;

    match &result {
        Ok(_) => info!("{source_kind_string} notifications successfully synced"),
        Err(err) => {
            error!("Failed to sync {source_kind_string} notifications: {err:?}")
        }
    };

    result
}

pub async fn sync_tasks_for_all_users(
    task_service: Arc<RwLock<TaskService>>,
    source: Option<TaskSyncSourceKind>,
) -> Result<(), UniversalInboxError> {
    let source_kind_string = source
        .map(|s| s.to_string())
        .unwrap_or_else(|| "all types of".to_string());
    info!("Syncing {source_kind_string} tasks for all users");
    let service = task_service.read().await;

    let result = service.sync_tasks_for_all_users(source).await;

    match &result {
        Ok(_) => info!("{source_kind_string} tasks successfully synced"),
        Err(err) => {
            error!("Failed to sync {source_kind_string} tasks: {err:?}")
        }
    };

    result
}
