use std::sync::Arc;

use apalis::prelude::*;
use opentelemetry::trace::Status;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{error, info};
use tracing_opentelemetry::OpenTelemetrySpanExt;

use crate::{
    integrations::slack::SlackService,
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService, task::service::TaskService,
        third_party::service::ThirdPartyItemService, UniversalInboxError,
    },
};

pub mod slack;
pub mod sync;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Serialize, Deserialize)]
pub enum UniversalInboxJob {
    SyncNotifications(sync::SyncNotificationsJob),
    SyncTasks(sync::SyncTasksJob),
    SlackPushEventCallback(slack::SlackPushEventCallbackJob),
}

impl UniversalInboxJob {
    pub fn name(&self) -> &'static str {
        match self {
            Self::SyncNotifications(_) => "SyncNotifications",
            Self::SyncTasks(_) => "SyncTasks",
            Self::SlackPushEventCallback(_) => "SlackPushEventCallback",
        }
    }
}

#[tracing::instrument(
    level = "debug",
    skip(
        job,
        task_id,
        notification_service,
        task_service,
        integration_connection_service,
        third_party_item_service,
        slack_service
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
