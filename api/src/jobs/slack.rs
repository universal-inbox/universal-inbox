use std::sync::Arc;

use anyhow::{anyhow, Context};
use apalis::prelude::Data;
use serde::{Deserialize, Serialize};
use slack_morphism::{
    events::{SlackReactionAddedEvent, SlackReactionRemovedEvent},
    prelude::{
        SlackEventCallbackBody, SlackPushEventCallback, SlackStarAddedEvent, SlackStarRemovedEvent,
    },
};
use tokio::sync::RwLock;
use tracing::{info, warn};

use universal_inbox::{
    integration_connection::{
        integrations::slack::{SlackConfig, SlackReactionConfig, SlackStarConfig, SlackSyncType},
        provider::{IntegrationProvider, IntegrationProviderKind},
    },
    third_party::item::ThirdPartyItemSourceKind,
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

    let (provider_user_id, third_party_item_source_kind) = match &event.0 {
        SlackPushEventCallback {
            event: SlackEventCallbackBody::StarAdded(SlackStarAddedEvent { user, .. }),
            ..
        }
        | SlackPushEventCallback {
            event: SlackEventCallbackBody::StarRemoved(SlackStarRemovedEvent { user, .. }),
            ..
        } => (user.to_string(), ThirdPartyItemSourceKind::SlackStar),
        SlackPushEventCallback {
            event: SlackEventCallbackBody::ReactionAdded(SlackReactionAddedEvent { user, .. }),
            ..
        }
        | SlackPushEventCallback {
            event: SlackEventCallbackBody::ReactionRemoved(SlackReactionRemovedEvent { user, .. }),
            ..
        } => (user.to_string(), ThirdPartyItemSourceKind::SlackReaction),
        _ => {
            return Err(UniversalInboxError::UnsupportedAction(format!(
                "Unsupported Slack event {event:?}"
            )))
        }
    };

    let Some(integration_connection) = integration_connection_service
        .read()
        .await
        .get_integration_connection_per_provider_user_id(
            &mut transaction,
            IntegrationProviderKind::Slack,
            provider_user_id.clone(),
        )
        .await?
    else {
        warn!("Integration connection not found for Slack user id {provider_user_id}");
        return Ok(());
    };

    let IntegrationProvider::Slack {
        config: slack_config,
    } = &integration_connection.provider
    else {
        return Err(UniversalInboxError::Unexpected(anyhow!(
            "Integration connection {} provider is supposed to be Slack",
            integration_connection.id
        )));
    };

    match slack_config {
        SlackConfig {
            star_config:
                SlackStarConfig {
                    sync_enabled: true,
                    sync_type: SlackSyncType::AsTasks(_),
                },
            ..
        } if third_party_item_source_kind == ThirdPartyItemSourceKind::SlackStar => {
            let task_service = task_service.read().await;

            fail_if_needed(
                task_service
                    .save_task_from_event(&mut transaction, event.0, integration_connection.user_id)
                    .await,
            )?;
        }
        SlackConfig {
            star_config:
                SlackStarConfig {
                    sync_enabled: true,
                    sync_type: SlackSyncType::AsNotifications,
                },
            ..
        } if third_party_item_source_kind == ThirdPartyItemSourceKind::SlackStar => {
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

        SlackConfig {
            reaction_config:
                SlackReactionConfig {
                    sync_enabled: true,
                    sync_type: SlackSyncType::AsTasks(_),
                    ..
                },
            ..
        } if third_party_item_source_kind == ThirdPartyItemSourceKind::SlackReaction => {
            let task_service = task_service.read().await;

            fail_if_needed(
                task_service
                    .save_task_from_event(&mut transaction, event.0, integration_connection.user_id)
                    .await,
            )?;
        }
        SlackConfig {
            reaction_config:
                SlackReactionConfig {
                    sync_enabled: true,
                    sync_type: SlackSyncType::AsNotifications,
                    ..
                },
            ..
        } if third_party_item_source_kind == ThirdPartyItemSourceKind::SlackReaction => {
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
        _ => {
            warn!(
                "Neither Slack star nor Slack reaction sync was enabled for integration connection {}",
                integration_connection.id
            );
            return Ok(());
        }
    };

    transaction
        .commit()
        .await
        .context("Failed to commit while handling a Slack event")?;

    Ok(())
}
