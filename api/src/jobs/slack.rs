use std::sync::Arc;

use anyhow::{anyhow, Context};
use apalis::prelude::Data;
use serde::{Deserialize, Serialize};
use slack_morphism::prelude::{
    SlackEventCallbackBody, SlackPushEventCallback, SlackStarAddedEvent, SlackStarRemovedEvent,
};
use tokio::sync::RwLock;
use tracing::info;

use universal_inbox::integration_connection::{
    integrations::slack::SlackSyncType,
    provider::{IntegrationProvider, IntegrationProviderKind},
};

use crate::universal_inbox::{
    integration_connection::service::IntegrationConnectionService,
    notification::{service::NotificationService, NotificationEventService},
    task::{service::TaskService, TaskEventService},
    UniversalInboxError,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct SlackPushEventCallbackJob(pub SlackPushEventCallback);

fn fail_if_needed<T>(
    result: Result<T, UniversalInboxError>,
) -> Result<Option<T>, UniversalInboxError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(UniversalInboxError::UnsupportedAction(_)) => Ok(None),
        Err(error) => Err(error),
    }
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
pub async fn handle_slack_push_event(
    event: SlackPushEventCallbackJob,
    notification_service: Data<Arc<RwLock<NotificationService>>>,
    task_service: Data<Arc<RwLock<TaskService>>>,
    integration_connection_service: Data<Arc<RwLock<IntegrationConnectionService>>>,
) -> Result<(), UniversalInboxError> {
    info!(?event, "Processing Slack push event");
    let service = notification_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while handling a Slack event")?;

    let provider_user_id = match &event.0 {
        SlackPushEventCallback {
            event: SlackEventCallbackBody::StarAdded(SlackStarAddedEvent { user, .. }),
            ..
        }
        | SlackPushEventCallback {
            event: SlackEventCallbackBody::StarRemoved(SlackStarRemovedEvent { user, .. }),
            ..
        } => user.to_string(),
        _ => {
            return Err(UniversalInboxError::UnsupportedAction(format!(
                "Unsupported Slack event {event:?}"
            )))
        }
    };

    let integration_connection = integration_connection_service
        .read()
        .await
        .get_integration_connection_per_provider_user_id(
            &mut transaction,
            IntegrationProviderKind::Slack,
            provider_user_id.clone(),
        )
        .await?
        .ok_or_else(|| {
            UniversalInboxError::UnsupportedAction(format!(
                "Integration connection not found for Slack user id {provider_user_id}"
            ))
        })?;

    let IntegrationProvider::Slack {
        config: slack_config,
    } = &integration_connection.provider
    else {
        return Err(UniversalInboxError::Unexpected(anyhow!(
            "Integration connection {} provider is supposed to be Slack",
            integration_connection.id
        )));
    };

    if !slack_config.sync_enabled {
        return Ok(());
    }

    if let SlackSyncType::AsTasks(_) = &slack_config.sync_type {
        let task_service = task_service.read().await;

        fail_if_needed(
            task_service
                .save_task_from_event(&mut transaction, event.0, integration_connection.user_id)
                .await,
        )?;
    } else {
        fail_if_needed(
            service
                .save_notification_from_event(
                    &mut transaction,
                    event.0,
                    integration_connection.user_id,
                )
                .await,
        )?;
    }

    transaction
        .commit()
        .await
        .context("Failed to commit while handling a Slack event")?;

    Ok(())
}
