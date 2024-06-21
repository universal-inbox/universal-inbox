use std::sync::Arc;

use apalis::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{error, info};

use crate::universal_inbox::{
    integration_connection::service::IntegrationConnectionService,
    notification::service::NotificationService, task::service::TaskService, UniversalInboxError,
};

pub mod slack;
pub mod sync;

#[derive(Debug, Serialize, Deserialize)]
pub enum UniversalInboxJob {
    SyncNotifications(sync::SyncNotificationsJob),
    SyncTasks(sync::SyncTasksJob),
    SlackPushEventCallback(slack::SlackPushEventCallbackJob),
}

impl Job for UniversalInboxJob {
    const NAME: &'static str = "universal-inbox:jobs:UniversalInboxJob";
}

#[tracing::instrument(
    level = "debug",
    skip(
        event,
        notification_service,
        task_service,
        integration_connection_service
    ),
    err
)]
pub async fn handle_universal_inbox_job(
    event: UniversalInboxJob,
    notification_service: Data<Arc<RwLock<NotificationService>>>,
    task_service: Data<Arc<RwLock<TaskService>>>,
    integration_connection_service: Data<Arc<RwLock<IntegrationConnectionService>>>,
) -> Result<(), UniversalInboxError> {
    let result = match event {
        UniversalInboxJob::SyncNotifications(job) => {
            sync::handle_sync_notifications(job, notification_service).await
        }
        UniversalInboxJob::SyncTasks(job) => sync::handle_sync_tasks(job, task_service).await,
        UniversalInboxJob::SlackPushEventCallback(job) => {
            slack::handle_slack_push_event(
                job,
                notification_service,
                task_service,
                integration_connection_service,
            )
            .await
        }
    };

    match result {
        Ok(_) => info!("Successfully executed job"),
        Err(err) => {
            error!("Failed to execute job: {err:?}");
            return Err(err);
        }
    };

    Ok(())
}
