use std::sync::Arc;

use apalis::prelude::Data;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use tracing_opentelemetry::OpenTelemetrySpanExt;
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

pub async fn handle_sync_notifications(
    event: SyncNotificationsJob,
    notification_service: Data<Arc<RwLock<NotificationService>>>,
) -> Result<(), UniversalInboxError> {
    let service = notification_service.read().await;
    let current_span = tracing::Span::current();
    if let Some(user_id) = event.user_id {
        current_span.set_attribute("user.id", user_id.to_string());
        if let Some(source) = event.source {
            current_span.set_attribute("synced_source", source.to_string());
            service
                .sync_notifications_with_transaction(source, user_id, false)
                .await?;
        } else {
            current_span.set_attribute("sync_all_sources", true);
            service.sync_all_notifications(user_id, false).await?;
        };
    } else {
        current_span.set_attribute("sync_all_users", true);
        if let Some(source) = event.source {
            current_span.set_attribute("synced_source", source.to_string());
        } else {
            current_span.set_attribute("sync_all_sources", true);
        };
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

pub async fn handle_sync_tasks(
    event: SyncTasksJob,
    task_service: Data<Arc<RwLock<TaskService>>>,
) -> Result<(), UniversalInboxError> {
    let service = task_service.read().await;
    let current_span = tracing::Span::current();
    if let Some(user_id) = event.user_id {
        current_span.set_attribute("user.id", user_id.to_string());
        if let Some(source) = event.source {
            current_span.set_attribute("synced_source", source.to_string());
            service
                .sync_tasks_with_transaction(source, user_id, false)
                .await?;
        } else {
            current_span.set_attribute("sync_all_sources", true);
            service.sync_all_tasks(user_id, false).await?;
        };
    } else {
        current_span.set_attribute("sync_all_users", true);
        if let Some(source) = event.source {
            current_span.set_attribute("synced_source", source.to_string());
        } else {
            current_span.set_attribute("sync_all_sources", true);
        };
        service
            .sync_tasks_for_all_users(event.source, false)
            .await?;
    }

    Ok(())
}
