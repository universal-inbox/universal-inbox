use std::sync::Arc;

use apalis::prelude::*;
use opentelemetry::trace::Status;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{error, info};
use tracing_opentelemetry::OpenTelemetrySpanExt;

use universal_inbox::{
    notification::{NotificationId, service::NotificationPatch},
    user::UserId,
};

use crate::{
    integrations::slack::SlackService,
    subscription::service::SubscriptionService,
    universal_inbox::{
        UniversalInboxError, integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService, task::service::TaskService,
        third_party::service::ThirdPartyItemService,
    },
};

pub mod slack;
pub mod sync;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Serialize, Deserialize)]
pub enum UniversalInboxJob {
    SyncNotifications(sync::SyncNotificationsJob),
    SyncTasks(sync::SyncTasksJob),
    SyncSubscriptions(sync::SyncSubscriptionsJob),
    SlackPushEventCallback(slack::SlackPushEventCallbackJob),
    ProcessNotificationSideEffects {
        notification_id: NotificationId,
        patch: NotificationPatch,
        user_id: UserId,
    },
}

impl UniversalInboxJob {
    pub fn name(&self) -> &'static str {
        match self {
            Self::SyncNotifications(_) => "SyncNotifications",
            Self::SyncTasks(_) => "SyncTasks",
            Self::SyncSubscriptions(_) => "SyncSubscriptions",
            Self::SlackPushEventCallback(_) => "SlackPushEventCallback",
            Self::ProcessNotificationSideEffects { .. } => "ProcessNotificationSideEffects",
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(
    level = "debug",
    skip(
        job,
        task_id,
        notification_service,
        task_service,
        integration_connection_service,
        third_party_item_service,
        slack_service,
        subscription_service
    ),
    fields(
        job.id = %task_id.to_string(),
        job.name = %job.name(),
    ),
    err
)]
pub async fn handle_universal_inbox_job(
    job: UniversalInboxJob,
    task_id: TaskId,
    notification_service: Data<Arc<RwLock<NotificationService>>>,
    task_service: Data<Arc<RwLock<TaskService>>>,
    integration_connection_service: Data<Arc<RwLock<IntegrationConnectionService>>>,
    third_party_item_service: Data<Arc<RwLock<ThirdPartyItemService>>>,
    slack_service: Data<Arc<SlackService>>,
    subscription_service: Data<Arc<SubscriptionService>>,
) -> Result<(), UniversalInboxError> {
    let current_span = tracing::Span::current();

    info!(
        job_id = task_id.to_string(),
        "Processing {} job",
        job.name()
    );
    let result = match job {
        UniversalInboxJob::SyncNotifications(job) => {
            sync::handle_sync_notifications(job, notification_service).await
        }
        UniversalInboxJob::SyncTasks(job) => sync::handle_sync_tasks(job, task_service).await,
        UniversalInboxJob::SyncSubscriptions(job) => {
            sync::handle_sync_subscriptions(job, subscription_service).await
        }
        UniversalInboxJob::SlackPushEventCallback(job) => {
            slack::handle_slack_push_event(
                job,
                notification_service,
                task_service,
                integration_connection_service,
                third_party_item_service,
                slack_service,
            )
            .await
        }
        UniversalInboxJob::ProcessNotificationSideEffects {
            notification_id,
            patch,
            user_id,
        } => {
            handle_process_notification_side_effects(
                notification_id,
                patch,
                user_id,
                notification_service,
            )
            .await
        }
    };

    match result {
        Ok(_) => {
            current_span.set_status(Status::Ok);
            info!(job_id = task_id.to_string(), "Successfully executed job");
            Ok(())
        }
        Err(err) => {
            current_span.set_status(Status::error(err.to_string()));
            error!(
                job_id = task_id.to_string(),
                "Failed to execute job: {err:?}"
            );
            Err(err)
        }
    }
}

#[tracing::instrument(
    level = "debug",
    skip(notification_service),
    fields(
        notification_id = notification_id.to_string(),
        user.id = user_id.to_string()
    ),
    err
)]
async fn handle_process_notification_side_effects(
    notification_id: NotificationId,
    patch: NotificationPatch,
    user_id: UserId,
    notification_service: Data<Arc<RwLock<NotificationService>>>,
) -> Result<(), UniversalInboxError> {
    let service = notification_service.read().await;
    let mut transaction = service.begin().await?;

    let mut notification = service
        .get_notification(&mut transaction, notification_id, user_id)
        .await?
        .ok_or_else(|| {
            UniversalInboxError::ItemNotFound(format!(
                "Notification with ID {} not found for user {}",
                notification_id, user_id
            ))
        })?;
    // Apply side effects by calling patch_notification with side effects enabled
    service
        .apply_notification_side_effects(&mut transaction, &mut notification, &patch, true, user_id)
        .await?;

    transaction
        .commit()
        .await
        .map_err(|e| UniversalInboxError::from(anyhow::Error::from(e)))?;
    Ok(())
}
