use std::{fmt, str::FromStr, sync::Arc};

use apalis::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{error, info};
use uuid::Uuid;

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

#[derive(Debug, Serialize, Deserialize)]
pub struct UniversalInboxJob {
    pub id: UniversalInboxJobId,
    pub payload: UniversalInboxJobPayload,
}

impl UniversalInboxJob {
    pub fn new(payload: UniversalInboxJobPayload) -> Self {
        Self {
            id: UniversalInboxJobId(Uuid::new_v4()),
            payload,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Copy, Clone, Eq, Hash)]
#[serde(transparent)]
pub struct UniversalInboxJobId(pub Uuid);

impl fmt::Display for UniversalInboxJobId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for UniversalInboxJobId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl From<UniversalInboxJobId> for Uuid {
    fn from(job_id: UniversalInboxJobId) -> Self {
        job_id.0
    }
}

impl TryFrom<String> for UniversalInboxJobId {
    type Error = uuid::Error;

    fn try_from(uuid: String) -> Result<Self, Self::Error> {
        Ok(Self(Uuid::parse_str(&uuid)?))
    }
}

impl FromStr for UniversalInboxJobId {
    type Err = uuid::Error;

    fn from_str(uuid: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(uuid)?))
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Serialize, Deserialize)]
pub enum UniversalInboxJobPayload {
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
        job,
        notification_service,
        task_service,
        integration_connection_service,
        third_party_item_service,
        slack_service
    ),
    fields(job_id = %job.id),
    err
)]
pub async fn handle_universal_inbox_job(
    job: UniversalInboxJob,
    notification_service: Data<Arc<RwLock<NotificationService>>>,
    task_service: Data<Arc<RwLock<TaskService>>>,
    integration_connection_service: Data<Arc<RwLock<IntegrationConnectionService>>>,
    third_party_item_service: Data<Arc<RwLock<ThirdPartyItemService>>>,
    slack_service: Data<Arc<SlackService>>,
) -> Result<(), UniversalInboxError> {
    let result = match job.payload {
        UniversalInboxJobPayload::SyncNotifications(job) => {
            sync::handle_sync_notifications(job, notification_service).await
        }
        UniversalInboxJobPayload::SyncTasks(job) => {
            sync::handle_sync_tasks(job, task_service).await
        }
        UniversalInboxJobPayload::SlackPushEventCallback(job) => {
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
        Ok(_) => info!("Successfully executed job"),
        Err(err) => {
            error!("Failed to execute job: {err:?}");
            return Err(err);
        }
    };

    Ok(())
}
