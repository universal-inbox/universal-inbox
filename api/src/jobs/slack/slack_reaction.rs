use std::sync::Arc;

use anyhow::anyhow;
use slack_morphism::prelude::*;
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use tracing::warn;

use universal_inbox::integration_connection::{
    integrations::slack::{SlackConfig, SlackReactionConfig, SlackSyncType},
    provider::{IntegrationProvider, IntegrationProviderKind},
};

use crate::universal_inbox::{
    integration_connection::service::IntegrationConnectionService,
    notification::{service::NotificationService, NotificationEventService},
    task::{service::TaskService, TaskEventService},
    UniversalInboxError,
};

pub async fn handle_slack_reaction_push_event(
    executor: &mut Transaction<'_, Postgres>,
    event: &SlackPushEventCallback,
    provider_user_id: String,
    notification_service: Arc<RwLock<NotificationService>>,
    task_service: Arc<RwLock<TaskService>>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
) -> Result<(), UniversalInboxError> {
    let Some(integration_connection) = integration_connection_service
        .read()
        .await
        .get_integration_connection_per_provider_user_id(
            executor,
            IntegrationProviderKind::Slack,
            provider_user_id.clone(),
        )
        .await?
    else {
        warn!("Validated integration connection not found for Slack user id {provider_user_id}");
        return Ok(());
    };

    let IntegrationProvider::Slack {
        config: slack_config,
        ..
    } = &integration_connection.provider
    else {
        return Err(UniversalInboxError::Unexpected(anyhow!(
            "Integration connection {} provider is supposed to be Slack",
            integration_connection.id
        )));
    };

    let user_id = integration_connection.user_id;
    let integration_connection_id = integration_connection.id;
    match slack_config {
        SlackConfig {
            reaction_config:
                SlackReactionConfig {
                    sync_enabled: true,
                    sync_type: SlackSyncType::AsTasks(_),
                    ..
                },
            ..
        } => task_service
            .read()
            .await
            .save_task_from_event(executor, event, user_id)
            .await
            .map(|_| ()),

        SlackConfig {
            reaction_config:
                SlackReactionConfig {
                    sync_enabled: true,
                    sync_type: SlackSyncType::AsNotifications,
                    ..
                },
            ..
        } => notification_service
            .read()
            .await
            .save_notification_from_event(executor, event, None, user_id)
            .await
            .map(|_| ()),

        _ => {
            warn!(
                "Slack reaction sync was not enabled for integration connection {integration_connection_id}"
            );
            Ok(())
        }
    }
}
