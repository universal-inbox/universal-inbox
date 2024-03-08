use std::sync::Arc;

use anyhow::Context;
use apalis::prelude::*;
use serde::{Deserialize, Serialize};
use slack_morphism::prelude::SlackPushEventCallback;
use tokio::sync::RwLock;
use tracing::info;

use crate::universal_inbox::{
    notification::{service::NotificationService, NotificationEventService},
    UniversalInboxError,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct SlackPushEventCallbackJob(pub SlackPushEventCallback);

impl Job for SlackPushEventCallbackJob {
    const NAME: &'static str = "universal-inbox:jobs:slack:SlackPushEventCallbackJob";
}

fn fail_if_needed<T>(
    result: Result<T, UniversalInboxError>,
) -> Result<Option<T>, UniversalInboxError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(UniversalInboxError::UnsupportedAction(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

#[tracing::instrument(level = "debug", skip(event, notification_service), err)]
pub async fn handle_slack_push_event(
    event: SlackPushEventCallbackJob,
    notification_service: Data<Arc<RwLock<NotificationService>>>,
) -> Result<(), UniversalInboxError> {
    info!(?event, "Processing Slack push event");
    let service = notification_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while creating a Slack notification")?;

    fail_if_needed(
        service
            .save_notification_from_event(&mut transaction, event.0)
            .await,
    )?;

    transaction
        .commit()
        .await
        .context("Failed to commit while creating Slack notification")?;
    Ok(())
}
